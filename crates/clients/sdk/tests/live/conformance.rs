//! The SDK conformance scenarios (`sdks/conformance/README.md`) against an
//! in-process dev server.

use std::time::Duration;

use rill_sdk::testing::DevServer;
use rill_sdk::{ApplyOptions, Code, Edit, EditBase, ErrorReason, LandStatus, LogStart, Root};
use rill_types::{OperationId, RepoPathBuf, TrunkSeq};

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

fn repo_id() -> String {
    format!("conf-{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

#[tokio::test(flavor = "multi_thread")]
async fn resources_and_errors() {
    let server = DevServer::start().await.unwrap();
    let client = server.client().await.unwrap();
    let id = repo_id();
    client.create_repo(&id, None).await.unwrap();
    let info = client.repo(id.clone()).info().await.unwrap();
    assert_eq!(info.name, id);
    let err = client.repo("absent").info().await.unwrap_err();
    assert_eq!(err.code(), Some(Code::NotFound));
    assert!(err.is(ErrorReason::NotFound));
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_read_back_and_retries_are_idempotent() {
    let server = DevServer::start().await.unwrap();
    let client = server.client().await.unwrap();
    let id = repo_id();
    client.create_repo(&id, None).await.unwrap();
    let repo = client.repo(id);
    let op = OperationId::new();
    let edit = || vec![Edit::set(path("hello.txt"), "hello\n")];
    let first = repo
        .apply_edits_with_op(
            op,
            EditBase::Root(Root::trunk()),
            edit(),
            ApplyOptions::message("hello"),
        )
        .await
        .unwrap();
    let root = Root::Version(first.hash);
    assert_eq!(
        &repo.read_file(&root, &path("hello.txt")).await.unwrap()[..],
        b"hello\n"
    );
    let listing = repo.list_dir(&root, &RepoPathBuf::root()).await.unwrap();
    assert_eq!(listing[0].0, "hello.txt");

    let again = repo
        .apply_edits_with_op(
            op,
            EditBase::Root(Root::trunk()),
            edit(),
            ApplyOptions::message("hello"),
        )
        .await
        .unwrap();
    assert_eq!(again.hash, first.hash);
    let reused = repo
        .apply_edits_with_op(
            op,
            EditBase::Root(Root::trunk()),
            edit(),
            ApplyOptions::message("another"),
        )
        .await
        .unwrap_err();
    assert_eq!(reused.code(), Some(Code::InvalidArgument));
    assert!(reused.is(ErrorReason::RequestIdReused));
}

#[tokio::test(flavor = "multi_thread")]
async fn histories_page() {
    let server = DevServer::start().await.unwrap();
    let client = server.client().await.unwrap();
    let id = repo_id();
    client.create_repo(&id, None).await.unwrap();
    let repo = client.repo(id);
    let first = repo
        .apply_edits(
            EditBase::Root(Root::trunk()),
            vec![Edit::set(path("a.txt"), "1\n")],
            ApplyOptions::message("one"),
        )
        .await
        .unwrap();
    let second = repo
        .apply_edits(
            EditBase::Root(Root::Version(first.hash)),
            vec![Edit::set(path("a.txt"), "2\n")],
            ApplyOptions::message("two"),
        )
        .await
        .unwrap();
    let start = LogStart::Version(second.hash);
    let page = repo
        .log_page(start.clone(), 1, None, false, None)
        .await
        .unwrap();
    assert_eq!(page.entries.len(), 1);
    let token = page.next_page_token.expect("a second page");
    let rest = repo
        .log_page(start, 1, None, false, Some(&token))
        .await
        .unwrap();
    assert_eq!(rest.entries[0].hash, first.hash);
    assert!(rest.next_page_token.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reviewed_change_lands_and_filters_find_it() {
    let server = DevServer::start().await.unwrap();
    let client = server.client().await.unwrap();
    let id = repo_id();
    client.create_repo(&id, None).await.unwrap();
    let repo = client.repo(id);
    let change = repo
        .apply_edits(
            EditBase::Root(Root::trunk()),
            vec![Edit::set(path("hello.txt"), "hello\n")],
            ApplyOptions::message("hello"),
        )
        .await
        .unwrap()
        .version
        .change_id;
    let reviews = repo.reviews();
    reviews.submit(change).await.unwrap();
    reviews
        .vote(change, "Verified", 1, None, None)
        .await
        .unwrap();
    reviews.vote(change, "Review", 2, None, None).await.unwrap();
    let ticket = reviews.request_land(change).await.unwrap();
    let landed = repo
        .stack()
        .await_land(&ticket.id, Duration::from_secs(30))
        .await
        .unwrap();
    assert_eq!(landed.status(), LandStatus::Landed);
    assert_eq!(landed.seq, Some(TrunkSeq(1)));
    assert_eq!(repo.trunk_head().await.unwrap().unwrap().0, TrunkSeq(1));

    let found = reviews
        .list(rill_sdk::ReviewFilter {
            status: vec![rill_types::ReviewStatus::Landed],
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(found.items.iter().any(|r| r.change_id == change));
    let err = client
        .review_svc()
        .list_reviews(rill_sdk::pb::ListReviewsRequest {
            parent: repo.resource().to_string(),
            filter: "colour = \"red\"".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(rill_sdk::Error::from(err).is(ErrorReason::InvalidFilter));
}

#[tokio::test(flavor = "multi_thread")]
async fn etags_guard_updates() {
    let server = DevServer::start().await.unwrap();
    let client = server.client().await.unwrap();
    let id = repo_id();
    client.create_repo(&id, None).await.unwrap();
    let admin = client.admin();
    let policy = admin.policy(&id, "submit").await.unwrap();
    assert!(!policy.etag.is_empty());
    let stale = admin
        .set_policy(&id, "submit", &policy.text, Some("stale"))
        .await
        .unwrap_err();
    assert_eq!(stale.code(), Some(Code::Aborted));
    assert!(stale.is(ErrorReason::EtagMismatch));
    admin
        .set_policy(&id, "submit", &policy.text, Some(&policy.etag))
        .await
        .unwrap();
}
