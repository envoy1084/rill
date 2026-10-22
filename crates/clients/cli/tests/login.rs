//! `rill login` through an identity provider's device flow, the stored
//! session the next commands use, its refresh, and `rill logout`.

use std::net::SocketAddr;

use rill_api::{Listeners, ServerConfig};
use rill_identity::keys::KeyServiceConfig;
use rill_identity::testing::{MockIdp, MockUser};
use rill_runtime::config::IdentityConfig;
use rill_types::{Principal, Timestamp};
use tokio_util::sync::CancellationToken;

async fn rill(args: &[&str]) -> (i32, String) {
    let mut out = Vec::new();
    let code = rill_cli::run_cli_with_args(args.iter().copied(), &mut out).await;
    (code, String::from_utf8(out).unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn login_keeps_a_session_that_refreshes_until_logout() {
    let dir = tempfile::tempdir().unwrap();
    let credentials = dir.path().join("credentials.json");
    // Only this test runs in this binary, so setting the variable is safe.
    std::env::set_var(rill_cli::credentials::CREDENTIALS_FILE_ENV, &credentials);

    let idp = MockIdp::start().await;
    idp.add_client("rill-web", Some("s"));
    idp.add_client("rill-cli", None);
    idp.add_user(MockUser::new("eve"));
    idp.auto_approve_devices("eve");

    let config = ServerConfig {
        identity: IdentityConfig {
            keys: KeyServiceConfig::Local {
                master_key_hex: Some("33".repeat(32)),
                master_key_file: None,
            },
            admins: vec!["root".into()],
            ..IdentityConfig::default()
        },
        ..ServerConfig::default()
    };
    let any: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listeners = Listeners::bind(any, any).await.unwrap();
    let (grpc, _) = listeners.addrs().unwrap();
    let shutdown = CancellationToken::new();
    let state = rill_api::build_state(config, shutdown.clone())
        .await
        .unwrap();
    let issuer = state.identity.issuer().unwrap();
    let serving = tokio::spawn(rill_api::serve_with_state(
        state,
        listeners,
        shutdown.clone(),
    ));
    let endpoint = format!("http://{grpc}");

    let root = issuer
        .issue(
            &Principal::human("root"),
            &rill_policy::TokenClaims::new("default", Timestamp::now(), 600_000),
            None,
        )
        .unwrap();
    let issuer_url = idp.issuer().to_string();
    let (code, out) = rill(&[
        "rill",
        "--endpoint",
        &endpoint,
        "--token",
        &root,
        "admin",
        "idp",
        "add",
        "mock",
        "--issuer",
        &issuer_url,
        "--client-id",
        "rill-web",
        "--client-secret",
        "s",
        "--cli-client-id",
        "rill-cli",
        "--auto-provision",
    ])
    .await;
    assert_eq!(code, 0, "{out}");

    let (code, out) = rill(&["rill", "--endpoint", &endpoint, "login"]).await;
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("eve"), "{out}");
    let (code, out) = rill(&[
        "rill",
        "--endpoint",
        &endpoint,
        "--token",
        &root,
        "--json",
        "admin",
        "user",
        "list",
    ])
    .await;
    assert_eq!(code, 0, "{out}");
    let users: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(users[0]["name"], "eve", "{users}");
    let (code, out) = rill(&[
        "rill",
        "--endpoint",
        &endpoint,
        "--token",
        &root,
        "--json",
        "admin",
        "key",
        "list",
    ])
    .await;
    assert_eq!(code, 0, "{out}");
    let keys: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(keys[0]["backend"], "local", "{keys}");
    let whoami = |endpoint: String| async move {
        let (code, out) =
            rill(&["rill", "--endpoint", &endpoint, "--json", "admin", "whoami"]).await;
        (code, out)
    };
    let (code, out) = whoami(endpoint.clone()).await;
    assert_eq!(code, 0, "{out}");
    let me: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(me["principal"]["name"], "eve");

    // A stored session near expiry refreshes itself and saves the rotated
    // refresh token.
    let store = rill_cli::credentials::store();
    let mut stored = store.load(&endpoint).unwrap().unwrap();
    let first_refresh = stored.refresh_token.clone();
    stored.expires_at = Timestamp::now().millis() + 1_000;
    store.save(&stored).unwrap();
    let client = rill_sdk::Client::connect(
        rill_sdk::ClientConfig::new(endpoint.clone()).with_session(stored, Some(store.clone())),
    )
    .await
    .unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let now = store.load(&endpoint).unwrap().unwrap();
        if now.refresh_token != first_refresh {
            assert!(now.expires_at > Timestamp::now().millis() + 60_000);
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "never refreshed");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(client.admin().whoami().await.unwrap().principal.name, "eve");

    let (code, out) = rill(&["rill", "--endpoint", &endpoint, "logout"]).await;
    assert_eq!(code, 0, "{out}");
    assert!(store.load(&endpoint).unwrap().is_none());
    let (code, _) = whoami(endpoint.clone()).await;
    assert_ne!(code, 0, "signed out");

    shutdown.cancel();
    serving.await.unwrap().unwrap();
}
