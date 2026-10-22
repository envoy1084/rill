//! Stack helpers: landing with backoff, submitting and syncing.

mod common;

use std::time::{Duration, Instant};

use common::{fake_hash, Harness, MockRepo, MockReview};
use rill_proto::rill::v1::LandStatus as PbLandStatus;
use rill_sdk::{Backoff, Error, LandStatus, Root};
use rill_types::{ChangeId, TrunkSeq};

fn fast() -> Backoff {
    Backoff::default()
        .with_min_delay(Duration::from_millis(5))
        .with_max_delay(Duration::from_millis(20))
        .without_max_times()
}

#[tokio::test]
async fn await_land_polls_until_landed() {
    let review = MockReview::with_script(&[
        PbLandStatus::Queued,
        PbLandStatus::Queued,
        PbLandStatus::Rebasing,
        PbLandStatus::Landed,
    ]);
    let h = Harness::start(MockRepo::new(), review, None).await;
    let stack = h.client.repo("main").stack().with_backoff(fast());

    let started = Instant::now();
    let progress = stack
        .await_land("t1", Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(progress.status(), LandStatus::Landed);
    assert_eq!(progress.seq, Some(TrunkSeq(7)));
    assert_eq!(progress.landed_hash, Some(fake_hash("landed")));
    assert_eq!(*h.review.polls.lock().unwrap(), 4);
    // Three waits of 5ms, 10ms and 20ms.
    assert!(started.elapsed() >= Duration::from_millis(35));
}

#[tokio::test]
async fn await_land_times_out_and_reports_failures() {
    let review = MockReview::with_script(&[PbLandStatus::Queued]);
    let h = Harness::start(MockRepo::new(), review, None).await;
    let stack = h.client.repo("main").stack().with_backoff(fast());
    let err = stack
        .await_land("t1", Duration::from_millis(30))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Timeout(_)));

    let review = MockReview::with_script(&[PbLandStatus::Queued, PbLandStatus::Failed]);
    let h = Harness::start(MockRepo::new(), review, None).await;
    let stack = h.client.repo("main").stack().with_backoff(fast());
    let err = stack
        .await_land("t2", Duration::from_secs(1))
        .await
        .unwrap_err();
    match err {
        Error::LandFailed {
            status, message, ..
        } => {
            assert_eq!(status, LandStatus::Failed);
            assert_eq!(message, "boom");
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn land_stack_lands_bottom_up() {
    let review = MockReview::with_script(&[PbLandStatus::Queued, PbLandStatus::Landed]);
    let h = Harness::start(MockRepo::new(), review, None).await;
    let stack = h.client.repo("main").stack().with_backoff(fast());
    let ids = [ChangeId::new(), ChangeId::new()];
    let landed = stack
        .land_stack(&ids, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(landed.len(), 2);
    assert_eq!(landed[0].change_id, ids[0]);
    assert_eq!(landed[1].change_id, ids[1]);
    assert_eq!(*h.review.land_requests.lock().unwrap(), ids.to_vec());
}

#[tokio::test]
async fn submit_stack_submits_each_then_sets_stack() {
    let h = Harness::start(MockRepo::new(), MockReview::default(), None).await;
    let stack = h.client.repo("main").stack();
    let ids = vec![ChangeId::new(), ChangeId::new(), ChangeId::new()];
    let states = stack.submit_stack(&ids).await.unwrap();
    assert_eq!(states.len(), 3);
    assert_eq!(states[0].stack, ids);
    assert_eq!(*h.review.submitted.lock().unwrap(), ids);
    assert_eq!(*h.review.stacks.lock().unwrap(), vec![ids]);
}

#[tokio::test]
async fn sync_stack_rebases_each_onto_previous_result() {
    let mock = MockRepo::new();
    let head = fake_hash("head");
    mock.set_trunk(9, head);
    let h = Harness::with_repo(mock).await;
    let stack = h.client.repo("main").stack();
    let ids = [ChangeId::new(), ChangeId::new()];

    let result = stack.sync_stack(&ids, &Root::trunk()).await.unwrap();
    assert!(result.is_clean());
    assert_eq!(result.changes.len(), 2);
    let rebases = h.repo.rebases.lock().unwrap();
    assert_eq!(rebases[0].1, head);
    assert_eq!(rebases[1].1, result.changes[0].to);
    assert_eq!(
        result.hashes(),
        vec![result.changes[0].to, result.changes[1].to]
    );
}
