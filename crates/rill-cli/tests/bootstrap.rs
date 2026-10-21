//! Bootstrapping a server without dev mode: the first token comes from
//! `rill api mint-token`, signed with the key the server then uses.

use std::net::SocketAddr;

use rill_api::{Listeners, ServerConfig};
use tokio_util::sync::CancellationToken;

async fn rill(args: &[&str]) -> (i32, String) {
    let mut out = Vec::new();
    let code = rill_cli::run_cli_with_args(args.iter().copied(), &mut out).await;
    (code, String::from_utf8(out).unwrap())
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

    let (code, out) = rill(&["rill", "api", "--config", &config_arg, "mint-token", "root"]).await;
    assert_eq!(code, 0, "{out}");
    let token = out.trim().to_string();
    // An agent token cannot outlive the agent limit.
    let (code, _) = rill(&[
        "rill",
        "api",
        "--config",
        &config_arg,
        "mint-token",
        "agent:bot",
        "--ttl",
        "86400",
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
    let (code, out) = rill(&[
        "rill",
        "--endpoint",
        &endpoint,
        "--token",
        &token,
        "--json",
        "admin",
        "whoami",
    ])
    .await;
    assert_eq!(code, 0, "{out}");
    let me: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(me["principal"]["name"], "root", "{me}");
    let (code, _) = rill(&[
        "rill",
        "--endpoint",
        &endpoint,
        "--token",
        "made-up",
        "admin",
        "whoami",
    ])
    .await;
    assert_ne!(code, 0, "an unsigned token is refused");

    shutdown.cancel();
    serving.await.unwrap().unwrap();
}
