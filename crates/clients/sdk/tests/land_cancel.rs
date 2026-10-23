//! A queued land request can be cancelled; a finished one cannot.

use std::time::Duration;

use rill_sdk::testing::DevServer;
use rill_sdk::{ApplyOptions, Code, Edit, EditBase, LandStatus, Root};
use rill_types::RepoPathBuf;

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queued_request_is_cancelled_and_never_lands() {
    let server = DevServer::start().await.unwrap();
    let client = server.client().await.unwrap();
    client.create_repo("cancel", None).await.unwrap();
    let repo = client.repo("cancel");
    let reviews = repo.reviews();
    let make = |name: &'static str| {
        let repo = repo.clone();
        async move {
            let change = repo
                .apply_edits(
                    EditBase::Root(Root::trunk()),
                    vec![Edit::set(path(name), name)],
                    ApplyOptions::message(name),
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
            change
        }
    };

    repo.trunk()
        .set_frozen(true, "hold the queue")
        .await
        .unwrap();
    let held = make("held.txt").await;
    let ticket = reviews.request_land(held).await.unwrap();
    let cancelled = reviews.cancel_land(&ticket.id, "not yet").await.unwrap();
    assert_eq!(cancelled.status(), LandStatus::Cancelled);
    assert!(cancelled.error.unwrap_or_default().contains("not yet"));
    repo.trunk().set_frozen(false, "").await.unwrap();

    let other = make("other.txt").await;
    let landed = repo
        .stack()
        .await_land(
            &reviews.request_land(other).await.unwrap().id,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    assert_eq!(landed.status(), LandStatus::Landed);
    let after = reviews.land_status(&ticket.id).await.unwrap();
    assert_eq!(
        after.status(),
        LandStatus::Cancelled,
        "the cancelled request stayed out"
    );

    let again = reviews
        .cancel_land(&landed.ticket.id, "")
        .await
        .unwrap_err();
    assert_eq!(again.code(), Some(Code::FailedPrecondition), "{again}");
}
