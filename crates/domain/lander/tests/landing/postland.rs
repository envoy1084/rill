//! Post-land verification, bisection and reverts.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rill_lander::{
    revert, AlwaysGreen, LandEvent, LandPolicy, LandStatus, Lander, NoopHooks, PostLandVerifier,
    VerificationStatus,
};
use rill_types::{Principal, RiskClass, TrunkSeq};

use crate::common::{fast_policy, request, Fixture};
use crate::doubles::{bad_file_verifier, selecting_verifier, CountingVerifier};

async fn land_chain(fx: &Fixture, lander: &Arc<Lander>, files: &[&[(&str, &str)]]) {
    let mut parent = fx.repo.trunk_head().await.unwrap().map(|(_, h)| h);
    for (i, set) in files.iter().enumerate() {
        let (hash, cv) = fx.commit(parent, set, &format!("step {i}")).await;
        let status = lander
            .land_now(request(&cv, RiskClass::Trivial))
            .await
            .unwrap();
        assert!(matches!(status, LandStatus::Landed { .. }), "{status:?}");
        parent = Some(hash);
    }
}

#[tokio::test]
async fn postland_marks_and_bisects() {
    let fx = Fixture::new().await;
    let lander = fx.lander(Arc::new(NoopHooks), bad_file_verifier(&fx), fast_policy());
    land_chain(
        &fx,
        &lander,
        &[
            &[("one", "1")],
            &[("two", "2")],
            &[("bad", "!")],
            &[("four", "4")],
            &[("five", "5")],
        ],
    )
    .await;
    let mut events = lander.subscribe();
    let worker = PostLandVerifier::new(lander.clone(), 8);

    // Batches start small: the first covers four landings.
    let sweep = worker.run_once().await.unwrap().unwrap();
    assert_eq!((sweep.from, sweep.to), (TrunkSeq(0), TrunkSeq(4)));
    assert!(!sweep.green);
    assert_eq!(sweep.culprit, Some(TrunkSeq(3)));
    let sweep = worker.run_once().await.unwrap().unwrap();
    assert_eq!((sweep.from, sweep.to), (TrunkSeq(4), TrunkSeq(5)));
    assert_eq!(sweep.culprit, None, "still red from the same culprit");
    assert_eq!(worker.cursor().await.unwrap(), TrunkSeq(5));

    let records = worker.recent().await.unwrap();
    let summary: Vec<_> = records
        .iter()
        .map(|r| {
            (
                r.seq.get(),
                r.status,
                r.culprit.map(|c| c.get()),
                r.inferred,
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (1, VerificationStatus::Green, None, false),
            (2, VerificationStatus::Green, None, false),
            (3, VerificationStatus::Red, Some(3), false),
            (4, VerificationStatus::Red, Some(3), false),
            (5, VerificationStatus::Red, Some(3), false),
        ]
    );
    let mut culprits = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let LandEvent::Culprit { seq } = event {
            culprits.push(seq);
        }
    }
    assert_eq!(culprits, vec![TrunkSeq(3)]);

    // Nothing new: no sweep.
    assert!(worker.run_once().await.unwrap().is_none());

    // A fix lands; the next batch is green again.
    let (_, head) = fx.head().await;
    let (_, fix) = fx.delete(head, "bad", "fix trunk@3").await;
    lander
        .land_now(request(&fix, RiskClass::Trivial))
        .await
        .unwrap();
    let sweep = worker.run_once().await.unwrap().unwrap();
    assert!(sweep.green);
    assert_eq!(
        worker.record_at(TrunkSeq(6)).await.unwrap().unwrap().status,
        VerificationStatus::Green
    );
}

#[tokio::test]
async fn postland_green_batch_needs_one_verification() {
    let fx = Fixture::new().await;
    let verifier = Arc::new(CountingVerifier {
        calls: AtomicUsize::new(0),
        green: true,
    });
    let lander = fx.lander(Arc::new(NoopHooks), verifier.clone(), fast_policy());
    land_chain(&fx, &lander, &[&[("a", "1")], &[("b", "2")], &[("c", "3")]]).await;
    let worker = PostLandVerifier::new(lander.clone(), 2);
    let sweep = worker.run_once().await.unwrap().unwrap();
    assert_eq!((sweep.from, sweep.to), (TrunkSeq(0), TrunkSeq(2)));
    assert_eq!(verifier.calls.load(Ordering::SeqCst), 1);
    let sweep = worker.run_once().await.unwrap().unwrap();
    assert_eq!((sweep.from, sweep.to), (TrunkSeq(2), TrunkSeq(3)));
    assert_eq!(verifier.calls.load(Ordering::SeqCst), 2);
    assert_eq!(worker.recent().await.unwrap().len(), 3);
}

#[tokio::test]
async fn a_worker_behind_trunk_catches_up_without_waiting_an_interval_per_batch() {
    let fx = Fixture::new().await;
    let lander = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    land_chain(&fx, &lander, &[&[("a", "1")], &[("b", "2")], &[("c", "3")]]).await;
    let worker = PostLandVerifier::new(lander.clone(), 1);
    let shutdown = tokio_util::sync::CancellationToken::new();
    let task = tokio::spawn(
        worker
            .clone()
            .run_forever(Duration::from_secs(1), shutdown.clone()),
    );
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        worker.cursor().await.unwrap(),
        TrunkSeq(3),
        "one interval verifies every batch trunk is ahead by"
    );
    shutdown.cancel();
    task.await.unwrap();
}

#[tokio::test]
async fn revert_restores_pre_land_tree() {
    let fx = Fixture::new().await;
    let lander = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    land_chain(
        &fx,
        &lander,
        &[
            &[("keep", "k"), ("edit", "v1")],
            &[("edit", "v2"), ("new", "n")],
        ],
    )
    .await;
    let before = fx
        .root(&fx.repo.trunk_at(TrunkSeq(1)).await.unwrap().unwrap())
        .await;

    let outcome = revert(&lander, TrunkSeq(2), "broke prod", Principal::human("bob"))
        .await
        .unwrap();
    assert_eq!(outcome.seq, TrunkSeq(3));
    assert_eq!(outcome.version.author, Principal::human("bob"));
    assert_eq!(
        outcome.version.provenance.tool.as_deref(),
        Some("lander:revert")
    );
    assert!(outcome.version.message.starts_with("Revert trunk@2"));

    let (_, head) = fx.head().await;
    assert_eq!(fx.root(&head).await, before);
}

#[tokio::test]
async fn auto_revert_after_grace_without_forward_fix() {
    let fx = Fixture::new().await;
    let policy = LandPolicy {
        auto_revert: true,
        revert_grace: Duration::from_millis(50),
        ..fast_policy()
    };
    let lander = fx.lander(Arc::new(NoopHooks), bad_file_verifier(&fx), policy);
    land_chain(
        &fx,
        &lander,
        &[
            &[("one", "1")],
            &[("bad", "!"), ("two", "2")],
            &[("three", "3")],
        ],
    )
    .await;
    let good = fx
        .root(&fx.repo.trunk_at(TrunkSeq(1)).await.unwrap().unwrap())
        .await;
    let worker = PostLandVerifier::new(lander.clone(), 8);
    let sweep = worker.run_once().await.unwrap().unwrap();
    assert_eq!(sweep.culprit, Some(TrunkSeq(2)));

    // The revert is a job, ready once the grace period is over; a lander
    // that replaced this one runs it just the same.
    let reverts = rill_jobs::Queue::new(fx.repo.clone(), rill_lander::postland::REVERT_QUEUE);
    assert_eq!(reverts.stats().await.unwrap().pending(), 1);
    let successor = fx.lander(Arc::new(NoopHooks), bad_file_verifier(&fx), fast_policy());
    lander.release().await;
    drop(lander);
    let runner = rill_jobs::Worker::new(
        reverts.clone(),
        rill_lander::postland::RevertHandler::new(successor.clone()),
    );
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(runner.run_once().await.unwrap());
    assert_eq!(reverts.stats().await.unwrap().pending(), 0);
    assert_eq!(fx.head().await.0, TrunkSeq(4), "the revert landed");
    let lander = successor;

    let (_, head) = fx.head().await;
    let root = fx.root(&head).await;
    assert!(fx.file(root, "bad").await.is_none());
    assert!(fx.file(root, "two").await.is_none());
    assert_eq!(fx.file(root, "three").await.as_deref(), Some("3"));
    assert_eq!(fx.file(root, "one").await, fx.file(good, "one").await);
    let cv = fx.repo.get_change_version(&head).await.unwrap().unwrap();
    assert_eq!(cv.author, Principal::service("lander"));
    assert_eq!(cv.provenance.tool.as_deref(), Some("lander:auto-revert"));
    assert!(rill_lander::has_forward_fix(&lander, TrunkSeq(2))
        .await
        .unwrap());
}

#[tokio::test]
async fn a_break_earlier_in_a_batch_is_caught_at_its_tip() {
    let fx = Fixture::new().await;
    let lander = fx.lander(Arc::new(NoopHooks), selecting_verifier(&fx), fast_policy());
    let worker = PostLandVerifier::with_full_every(lander.clone(), 8, Duration::from_secs(3600));
    land_chain(&fx, &lander, &[&[("one", "1")]]).await;
    assert!(
        worker.run_once().await.unwrap().unwrap().full,
        "the first sweep runs everything"
    );
    // The tip does not touch `bad`: verified only against its own parent it
    // would skip the check and pass.
    land_chain(&fx, &lander, &[&[("bad", "!")], &[("three", "3")]]).await;
    let sweep = worker.run_once().await.unwrap().unwrap();
    assert!(!sweep.full);
    assert!(!sweep.green);
    assert_eq!(sweep.culprit, Some(TrunkSeq(2)));
}

#[tokio::test]
async fn a_flaky_result_passes_and_finds_no_culprit() {
    let fx = Fixture::new().await;
    let verifier = Arc::new(rill_lander::FnVerifier(|_| async {
        Ok(rill_lander::VerificationOutcome {
            flaky: true,
            ..rill_lander::VerificationOutcome::green("passed on a re-run")
        })
    }));
    let lander = fx.lander(Arc::new(NoopHooks), verifier, fast_policy());
    land_chain(&fx, &lander, &[&[("a", "1")], &[("b", "2")]]).await;
    let worker = PostLandVerifier::new(lander.clone(), 8);
    let sweep = worker.run_once().await.unwrap().unwrap();
    assert!(sweep.green && sweep.culprit.is_none());
    let tip = worker.record_at(TrunkSeq(2)).await.unwrap().unwrap();
    assert_eq!(tip.status, VerificationStatus::Flaky);
    let reverts = rill_jobs::Queue::new(fx.repo.clone(), rill_lander::postland::REVERT_QUEUE);
    assert_eq!(
        reverts.stats().await.unwrap().pending(),
        0,
        "nothing to revert"
    );
}
