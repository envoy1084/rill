//! TLS end to end: clients check the server's certificate, and a server
//! with a client CA refuses connections without a client certificate.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, ExtendedKeyUsagePurpose, IsCa, KeyPair,
};
use rill_api::{AppState, Listeners, ServerConfig, TlsConfig};
use rill_sdk::{Client, ClientConfig, TlsOptions};
use tokio_util::sync::CancellationToken;

struct Pki {
    dir: tempfile::TempDir,
}

impl Pki {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca = CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap();
        std::fs::write(dir.path().join("ca.pem"), ca.pem()).unwrap();
        for (name, usage) in [
            ("server", ExtendedKeyUsagePurpose::ServerAuth),
            ("client", ExtendedKeyUsagePurpose::ClientAuth),
        ] {
            let key = KeyPair::generate().unwrap();
            let mut params =
                CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()])
                    .unwrap();
            params.extended_key_usages = vec![usage];
            let cert = params.signed_by(&key, &ca).unwrap();
            std::fs::write(dir.path().join(format!("{name}.pem")), cert.pem()).unwrap();
            std::fs::write(dir.path().join(format!("{name}.key")), key.serialize_pem()).unwrap();
        }
        Pki { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn server_tls(&self, mutual: bool) -> TlsConfig {
        TlsConfig {
            cert: self.path("server.pem"),
            key: self.path("server.key"),
            client_ca: mutual.then(|| self.path("ca.pem")),
        }
    }

    fn client_tls(&self, with_cert: bool) -> TlsOptions {
        TlsOptions {
            ca_file: Some(self.path("ca.pem")),
            client_cert: with_cert.then(|| self.path("client.pem")),
            client_key: with_cert.then(|| self.path("client.key")),
        }
    }
}

struct Server {
    grpc: u16,
    http: u16,
    signal: CancellationToken,
    _state: Arc<AppState>,
}

async fn serve(tls: TlsConfig) -> Server {
    let config = ServerConfig {
        dev_mode: true,
        tls: Some(tls),
        ..ServerConfig::default()
    };
    let signal = CancellationToken::new();
    let state =
        AppState::with_stores(config, rill_store::Stores::memory(), CancellationToken::new())
            .unwrap();
    let listeners = Listeners::bind(
        "127.0.0.1:0".parse().unwrap(),
        "127.0.0.1:0".parse().unwrap(),
    )
    .await
    .unwrap();
    let (grpc, http) = listeners.addrs().unwrap();
    tokio::spawn(rill_api::serve_until(state.clone(), listeners, signal.clone()));
    Server {
        grpc: grpc.port(),
        http: http.port(),
        signal,
        _state: state,
    }
}

async fn whoami(endpoint: String, tls: TlsOptions) -> rill_sdk::Result<String> {
    let config = ClientConfig::new(endpoint)
        .with_tls(tls)
        .with_timeout(Some(Duration::from_secs(5)));
    let client = Client::connect(config).await?;
    Ok(client.admin().whoami().await?.principal.name)
}

#[tokio::test(flavor = "multi_thread")]
async fn clients_check_the_server_certificate() {
    let pki = Pki::new();
    let server = serve(pki.server_tls(false)).await;
    let https = format!("https://localhost:{}", server.grpc);

    assert_eq!(whoami(https.clone(), pki.client_tls(false)).await.unwrap(), "dev");
    assert!(
        whoami(https, TlsOptions::default()).await.is_err(),
        "a server signed by an unknown CA is refused"
    );
    assert!(
        whoami(format!("http://localhost:{}", server.grpc), TlsOptions::default())
            .await
            .is_err(),
        "plain text gets nowhere"
    );
    server.signal.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn mutual_tls_refuses_clients_without_a_certificate() {
    let pki = Pki::new();
    let server = serve(pki.server_tls(true)).await;
    let https = format!("https://localhost:{}", server.grpc);
    assert!(whoami(https.clone(), pki.client_tls(false)).await.is_err());
    assert_eq!(whoami(https, pki.client_tls(true)).await.unwrap(), "dev");

    let ca = reqwest::Certificate::from_pem(&std::fs::read(pki.path("ca.pem")).unwrap()).unwrap();
    let mut identity = std::fs::read(pki.path("client.pem")).unwrap();
    identity.extend(std::fs::read(pki.path("client.key")).unwrap());
    let readyz = format!("https://localhost:{}/readyz", server.http);
    let anonymous = reqwest::Client::builder()
        .add_root_certificate(ca.clone())
        .build()
        .unwrap();
    assert!(anonymous.get(&readyz).send().await.is_err());
    let known = reqwest::Client::builder()
        .add_root_certificate(ca)
        .identity(reqwest::Identity::from_pem(&identity).unwrap())
        .build()
        .unwrap();
    assert_eq!(known.get(&readyz).send().await.unwrap().status(), 200);
    server.signal.cancel();
}
