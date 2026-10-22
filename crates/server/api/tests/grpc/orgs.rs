//! Repositories live in orgs and are addressed as `org/repo`.

use super::*;
use common::names;

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
        request_id: String::new(),
        org_id: name.into(),
        description: None,
    };
    let err = bot
        .orgs
        .create_org(create_org("botcorp"))
        .await
        .unwrap_err();
    assert_eq!(err.code, Code::PermissionDenied);
    alice.orgs.create_org(create_org("acme")).await.unwrap();
    let dup = alice.orgs.create_org(create_org("acme")).await.unwrap_err();
    assert_eq!(dup.code, Code::AlreadyExists);

    let create_repo = |org: &str, repo: &str| pb::CreateRepoRequest {
        request_id: String::new(),
        parent: names::org(org),
        repo_id: repo.into(),
        repo: None.into(),
    };
    let missing = alice
        .repo
        .create_repo(create_repo("ghost", "app"))
        .await
        .unwrap_err();
    assert_eq!(missing.code, Code::NotFound);
    let scoped = alice
        .repo
        .create_repo(create_repo("acme", "app"))
        .await
        .unwrap()
        .into_owned()
        .repo
        .unwrap();
    let plain = alice
        .repo
        .create_repo(create_repo("default", "app"))
        .await
        .unwrap()
        .into_owned()
        .repo
        .unwrap();
    assert_eq!(
        (scoped.org.as_str(), plain.org.as_str()),
        ("acme", "default")
    );
    assert_ne!(scoped.uid, plain.uid);

    alice
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            request_id: String::new(),
            parent: names::repo("acme/app"),
            base: None.into(),
            edits: vec![set_edit("README.md", b"acme\n")],
            message: "first".into(),
            intent: String::new(),
            provenance: None.into(),
            change: String::new(),
        })
        .await
        .unwrap();
    let listed = alice
        .content
        .list_directory(pb::ListDirectoryRequest {
            repo: names::repo("app"),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert!(
        listed.entries.is_empty(),
        "the default org's app is untouched"
    );

    let orgs = alice
        .orgs
        .list_orgs(pb::ListOrgsRequest::default())
        .await
        .unwrap()
        .into_owned();
    let ids: Vec<_> = orgs.orgs.iter().map(|o| o.org_id.as_str()).collect();
    assert_eq!(ids, vec!["acme", "default"]);
    server.stop().await;
}
