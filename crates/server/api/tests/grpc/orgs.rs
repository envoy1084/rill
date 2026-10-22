//! Repositories live in orgs and are addressed as `org/repo`.

use super::*;

#[tokio::test]
async fn repositories_are_addressed_within_their_org() {
    let mut config = ServerConfig::default();
    config.identity.admins = vec!["alice".into()];
    let server = TestServer::start(config).await;
    let alice_token = server.token_for(&rill_types::Principal::human("alice"));
    let bot_token = server.token_for(&rill_types::Principal::agent("bot"));
    let alice = clients(&server, Some(&alice_token)).await;
    let bot = clients(&server, Some(&bot_token)).await;

    let create_org = |name: &str| pb::CreateOrgRequest {
        operation_id: Bytes::new(),
        name: name.into(),
        description: None,
    };
    let err = bot
        .admin
        .create_org(create_org("botcorp"))
        .await
        .unwrap_err();
    assert_eq!(err.code, Code::PermissionDenied);
    alice.admin.create_org(create_org("acme")).await.unwrap();
    let dup = alice
        .admin
        .create_org(create_org("acme"))
        .await
        .unwrap_err();
    assert_eq!(dup.code, Code::AlreadyExists);

    let create_repo = |repo: &str| pb::CreateRepoRequest {
        operation_id: Bytes::new(),
        repo: repo.into(),
        description: None,
        windows_safe: false,
    };
    let missing = alice
        .repo
        .create_repo(create_repo("ghost/app"))
        .await
        .unwrap_err();
    assert_eq!(missing.code, Code::NotFound);
    let scoped = alice
        .repo
        .create_repo(create_repo("acme/app"))
        .await
        .unwrap()
        .into_owned()
        .repo
        .unwrap();
    let plain = alice
        .repo
        .create_repo(create_repo("app"))
        .await
        .unwrap()
        .into_owned()
        .repo
        .unwrap();
    assert_eq!(
        (scoped.org.as_str(), plain.org.as_str()),
        ("acme", "default")
    );
    assert_ne!(scoped.id, plain.id);

    alice
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "acme/app".into(),
            base: None.into(),
            edits: vec![set_edit("README.md", b"acme\n")],
            message: "first".into(),
            intent: None,
            provenance: None.into(),
            change_id: None,
        })
        .await
        .unwrap();
    let listed = alice
        .repo
        .list_dir(pb::ListDirRequest {
            repo: "app".into(),
            root: None.into(),
            path: String::new(),
        })
        .await
        .unwrap()
        .into_owned();
    assert!(
        listed.entries.is_empty(),
        "the default org's app is untouched"
    );

    let orgs = alice
        .admin
        .list_orgs(pb::ListOrgsRequest {})
        .await
        .unwrap()
        .into_owned();
    let names: Vec<_> = orgs.orgs.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, vec!["acme", "default"]);
    server.stop().await;
}
