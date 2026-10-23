//! Bootstrapping a server without dev mode: the first token comes from
//! `rill-server mint-token`, signed with the key the server then uses.

use std::net::SocketAddr;

use rill_api::{Listeners, ServerConfig};
use tokio_util::sync::CancellationToken;

async fn rill_server(args: &[&str]) -> (i32, String) {
    let mut out = Vec::new();
    let code = rill_server::run_with_args(args.iter().copied(), &mut out).await;
    (code, String::from_utf8(out).unwrap())
}

async fn whoami(endpoint: &str, token: &str) -> rill_sdk::Result<String> {
    let mut config = rill_sdk::ClientConfig::new(endpoint);
    config.token = Some(token.to_string());
    let client = rill_sdk::Client::connect(config).await?;
    Ok(client.admin().whoami().await?.principal.name)
}

#[tokio::test]
async fn a_minted_token_signs_in_to_a_server_on_the_same_store() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("rill.toml");
    let master = "42".repeat(32);
    std::fs::write(
        &config_path,
        format!(
            r#"
            [storage]
            kind = "local"
            dir = "{}"
            [identity]
            admins = ["root"]
            [identity.keys]
            backend = "local"
            master_key_hex = "{master}"
            "#,
            dir.path().join("data").display()
        ),
    )
    .unwrap();
    let config_arg = config_path.display().to_string();

    let (code, out) =
        rill_server(&["rill-server", "mint-token", "root", "--config", &config_arg]).await;
    assert_eq!(code, 0, "{out}");
    let token = out.trim().to_string();
    // An agent token cannot outlive the agent limit.
    let (code, _) = rill_server(&[
        "rill-server",
        "mint-token",
        "agent:bot",
        "--ttl",
        "86400",
        "--config",
        &config_arg,
    ])
    .await;
    assert_ne!(code, 0);

    let config = ServerConfig::load(Some(&config_path)).unwrap();
    assert!(!config.dev_mode);
    let any: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listeners = Listeners::bind(any, any).await.unwrap();
    let (grpc, _) = listeners.addrs().unwrap();
    let shutdown = CancellationToken::new();
    let state = rill_api::build_state(config, shutdown.clone())
        .await
        .unwrap();
    let serving = tokio::spawn(rill_api::serve_with_state(
        state,
        listeners,
        shutdown.clone(),
    ));

    let endpoint = format!("http://{grpc}");
    assert_eq!(whoami(&endpoint, &token).await.unwrap(), "root");
    assert!(
        whoami(&endpoint, "made-up").await.is_err(),
        "an unsigned token is refused"
    );

    shutdown.cancel();
    serving.await.unwrap().unwrap();
}
