//! TLS on both listeners from PEM files: the server's certificate chain
//! and key, and optionally a CA whose client certificates every
//! connection must present (mutual TLS between services).

use std::path::Path;
use std::sync::Arc;

use rustls::server::WebPkiClientVerifier;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tonic::transport::{Certificate, Identity, ServerTlsConfig};

use crate::config::TlsConfig;
use crate::error::{Error, Result};

/// The gRPC listener's TLS.
pub fn grpc(config: &TlsConfig) -> Result<ServerTlsConfig> {
    let identity = Identity::from_pem(read(&config.cert)?, read(&config.key)?);
    let mut tls = ServerTlsConfig::new().identity(identity);
    if let Some(ca) = &config.client_ca {
        tls = tls.client_ca_root(Certificate::from_pem(read(ca)?));
    }
    Ok(tls)
}

/// The HTTP listener's TLS.
pub fn http(config: &TlsConfig) -> Result<axum_server::tls_rustls::RustlsConfig> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let certs = CertificateDer::pem_file_iter(&config.cert)
        .and_then(|certs| certs.collect::<std::result::Result<Vec<_>, _>>())
        .map_err(|e| tls_err(&config.cert, e))?;
    let key = PrivateKeyDer::from_pem_file(&config.key).map_err(|e| tls_err(&config.key, e))?;
    let builder = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Config(format!("tls: {e}")))?;
    let builder = match &config.client_ca {
        Some(ca) => {
            let mut roots = rustls::RootCertStore::empty();
            for cert in CertificateDer::pem_file_iter(ca).map_err(|e| tls_err(ca, e))? {
                roots
                    .add(cert.map_err(|e| tls_err(ca, e))?)
                    .map_err(|e| Error::Config(format!("tls client_ca: {e}")))?;
            }
            let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider)
                .build()
                .map_err(|e| Error::Config(format!("tls client_ca: {e}")))?;
            builder.with_client_cert_verifier(verifier)
        }
        None => builder.with_no_client_auth(),
    };
    let mut server = builder
        .with_single_cert(certs, key)
        .map_err(|e| Error::Config(format!("tls: {e}")))?;
    server.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(server)))
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error::Config(format!("tls: reading {}: {e}", path.display())))
}

fn tls_err(path: &Path, e: impl std::fmt::Display) -> Error {
    Error::Config(format!("tls: {}: {e}", path.display()))
}
