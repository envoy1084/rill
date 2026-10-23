//! A reader of one directory through the SDK and CLI against a real
//! server: listings, file reads (which need the read grants listings
//! return) and `rill access`.

use std::net::SocketAddr;

use rill_api::{Listeners, ServerConfig};
use rill_sdk::{ApplyOptions, Client, ClientConfig, Edit, EditBase, Root};
use rill_types::{Principal, RepoPathBuf};
use tokio_util::sync::CancellationToken;

async fn rill(args: &[&str]) -> (i32, String) {
    let mut out = Vec::new();
    let code = rill_cli::run_cli_with_args(args.iter().copied(), &mut out).await;
    (code, String::from_utf8(out).unwrap())
}

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

#[tokio::test]
async fn a_directory_reader_reads_its_directory_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("rill.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
            [storage]
            kind = "local"
            dir = "{}"
            [identity.keys]
            backend = "local"
            master_key_hex = "{}"
            "#,
            dir.path().join("data").display(),
            "17".repeat(32)
        ),
    )
    .unwrap();
    let config = ServerConfig::load(Some(&config_path)).unwrap();
    let mint = |who: &'static str| {
        let config = config.clone();
        async move {
            let human = rill_types::Principal::human(who);
            rill_runtime::identity::mint_token(&config, &human, rill_store::DEFAULT_ORG, None)
                .await
                .unwrap()
        }
    };
    let (ann, bob) = (mint("ann").await, mint("bob").await);

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
    let client = |token: &str| {
        let mut config = ClientConfig::new(endpoint.clone());
        config.token = Some(token.to_string());
        Client::connect(config)
    };

    let owner = client(&ann).await.unwrap();
    owner.create_repo("acl", None).await.unwrap();
    // The SDK edits on top of a root; the first version starts from
    // nothing over the raw RPC.
    let first = owner
        .change_svc()
        .apply_edits(rill_proto::rill::v1::ApplyEditsRequest {
            parent: rill_sdk::names::repo("acl"),
            message: "start".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap()
        .hash;
    let first = rill_types::ObjectHash::from_slice(&first).unwrap();
    let public = "public\n".repeat(100);
    let secret = "secret\n".repeat(100);
    let version = owner
        .repo("acl")
        .apply_edits(
            EditBase::Root(Root::Version(first)),
            vec![
                Edit::set(path("pub/a.txt"), public.clone()),
                Edit::set(path("secret/s.txt"), secret.clone()),
            ],
            ApplyOptions {
                message: "seed".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .hash;
    let bob_id = Principal::human("bob").id.to_string();
    let (code, out) = rill(&[
        "rill",
        "--endpoint",
        &endpoint,
        "--token",
        &ann,
        "--repo",
        "acl",
        "access",
        "grant",
        &format!("principal:{bob_id}"),
        "reader",
        "pub",
    ])
    .await;
    assert_eq!(code, 0, "{out}");

    let reader = client(&bob).await.unwrap().repo("acl");
    let at = Root::Version(version);
    let names: Vec<String> = reader
        .list_dir(&at, &RepoPathBuf::root())
        .await
        .unwrap()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["pub"]);
    // Reading a file resolves it first, and the resolution's grant opens
    // its content.
    let read = reader.read_file(&at, &path("pub/a.txt")).await.unwrap();
    assert_eq!(read, public.as_bytes());
    let refused = reader.read_file(&at, &path("secret/s.txt")).await;
    assert!(refused.is_err(), "{refused:?}");

    let (code, out) = rill(&[
        "rill",
        "--endpoint",
        &endpoint,
        "--token",
        &bob,
        "--repo",
        "acl",
        "--json",
        "access",
        "check",
        "reader",
        "secret",
    ])
    .await;
    assert_eq!(code, 0, "{out}");
    let check: serde_json::Value = serde_json::from_str(&out).unwrap();
    let check = check["result"].clone();
    assert_eq!(check["allowed"], false, "{check}");

    shutdown.cancel();
    serving.await.unwrap().unwrap();
}
