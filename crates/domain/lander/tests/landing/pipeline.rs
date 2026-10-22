//! Landing requests: clean, rebased, conflicting, refused and red.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rill_lander::land::followups;
use rill_lander::{
    AlwaysGreen, FnVerifier, LandEvent, LandStatus, NoopHooks, VerificationOutcome, VerifyRequest,
    VerifyStage,
};
use rill_types::{RepoPathBuf, RiskClass, TrunkSeq};

use crate::common::{fast_policy, request, Fixture};
use crate::doubles::{CountingVerifier, RecordingHooks, RefusingHooks};

#[tokio::test]
async fn lands_on_empty_trunk() {
    let fx = Fixture::new().await;
    let hooks = Arc::new(RecordingHooks::default());
    let lander = fx.lander(hooks.clone(), Arc::new(AlwaysGreen), fast_policy());
    let mut events = lander.subscribe();

    let (hash, cv) = fx.commit(None, &[("a.txt", "a")], "init").await;
    let status = lander
        .land_now(request(&cv, RiskClass::Normal))
        .await
        .unwrap();
    assert_eq!(status, LandStatus::Landed { seq: TrunkSeq(1) });
    assert_eq!(fx.head().await, (TrunkSeq(1), hash));
    assert_eq!(
        hooks.landed.lock().await.as_slice(),
        &[(cv.change_id, TrunkSeq(1))]
    );

    let landings = lander.list_landings().await.unwrap();
    assert_eq!(landings.len(), 1);
    assert_eq!(landings[0].version, hash);
    assert_eq!(landings[0].seq, TrunkSeq(1));

    let mut saw_landed = false;
    while let Ok(event) = events.try_recv() {
        if let LandEvent::Landed { seq, version, .. } = event {
            assert_eq!((seq, version), (TrunkSeq(1), hash));
            saw_landed = true;
        }
    }
    assert!(saw_landed);

    // Landing the same version again is refused rather than duplicated.
    let status = lander
        .land_now(request(&cv, RiskClass::Normal))
        .await
        .unwrap();
    assert!(matches!(status, LandStatus::Failed { .. }));
}

#[tokio::test]
async fn request_the_review_refuses_does_not_land() {
    let fx = Fixture::new().await;
    let lander = fx.lander(
        Arc::new(RefusingHooks),
        Arc::new(AlwaysGreen),
        fast_policy(),
    );
    let (_, cv) = fx.commit(None, &[("a.txt", "a")], "init").await;
    let status = lander
        .land_now(request(&cv, RiskClass::Normal))
        .await
        .unwrap();
    let LandStatus::Failed { reason } = status else {
        panic!("expected a refusal, got {status:?}");
    };
    assert!(reason.contains("no longer landable"), "{reason}");
    assert!(fx.repo.trunk_head().await.unwrap().is_none());
}

#[tokio::test]
async fn independent_changes_land_in_one_run() {
    let fx = Fixture::new().await;
    let lander = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    let (base, _) = fx.commit(None, &[("base", "0")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();

    let (_, a) = fx.commit(Some(base), &[("a.txt", "a")], "a").await;
    let (_, b) = fx.commit(Some(base), &[("b.txt", "b")], "b").await;
    let ta = lander
        .enqueue(request(&a, RiskClass::Trivial))
        .await
        .unwrap();
    let tb = lander
        .enqueue(request(&b, RiskClass::Trivial))
        .await
        .unwrap();
    assert_eq!(lander.run_once().await.unwrap(), 2);

    let mut seqs = vec![
        match lander.queue().status(ta).await.unwrap().unwrap() {
            LandStatus::Landed { seq } => seq,
            other => panic!("a: {other:?}"),
        },
        match lander.queue().status(tb).await.unwrap().unwrap() {
            LandStatus::Landed { seq } => seq,
            other => panic!("b: {other:?}"),
        },
    ];
    seqs.sort();
    assert_eq!(seqs, vec![TrunkSeq(2), TrunkSeq(3)]);

    let (seq, head) = fx.head().await;
    assert_eq!(seq, TrunkSeq(3));
    let root = fx.root(&head).await;
    assert_eq!(fx.file(root, "a.txt").await.as_deref(), Some("a"));
    assert_eq!(fx.file(root, "b.txt").await.as_deref(), Some("b"));
    // Every trunk position has a record, the directly landed base too.
    assert_eq!(lander.list_landings().await.unwrap().len(), 3);

    // Both landed in one transaction, which queued their follow-up work: a
    // derive job each and one index job for the new head.
    let derive = rill_jobs::Queue::new(fx.repo.clone(), followups::DERIVE_QUEUE);
    let index = rill_jobs::Queue::new(fx.repo.clone(), followups::INDEX_QUEUE);
    assert_eq!(derive.stats().await.unwrap().enqueued, 2);
    assert_eq!(index.stats().await.unwrap().enqueued, 1);
    let mut derived = Vec::new();
    while let Some(claim) = derive.claim("test").await.unwrap() {
        derived.push(rill_types::ChangeVersionHash::from_slice(&claim.job.payload).unwrap());
        derive.complete(&claim).await.unwrap();
    }
    derived.sort();
    let mut landed = vec![fx.repo.trunk_at(TrunkSeq(2)).await.unwrap().unwrap(), head];
    landed.sort();
    assert_eq!(derived, landed);
}

#[tokio::test]
async fn change_behind_trunk_is_rebased() {
    let fx = Fixture::new().await;
    let lander = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    let (base, _) = fx.commit(None, &[("f", "base\n")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();

    let (x_hash, x) = fx.commit(Some(base), &[("x", "x")], "x").await;
    let (y_hash, y) = fx.commit(Some(base), &[("y", "y")], "y").await;
    assert_eq!(
        lander.land_now(request(&y, RiskClass::Low)).await.unwrap(),
        LandStatus::Landed { seq: TrunkSeq(2) }
    );
    assert_eq!(
        lander.land_now(request(&x, RiskClass::Low)).await.unwrap(),
        LandStatus::Landed { seq: TrunkSeq(3) }
    );

    let (_, head) = fx.head().await;
    assert_ne!(head, x_hash);
    let landed = fx.repo.get_change_version(&head).await.unwrap().unwrap();
    assert_eq!(landed.change_id, x.change_id);
    assert_eq!(landed.predecessor, Some(x_hash));
    assert_eq!(landed.parents, vec![y_hash]);
    let root = fx.root(&head).await;
    assert_eq!(fx.file(root, "x").await.as_deref(), Some("x"));
    assert_eq!(fx.file(root, "y").await.as_deref(), Some("y"));

    let versions = fx.repo.change_versions(&x.change_id).await.unwrap();
    assert_eq!(versions, vec![x_hash, head]);
    let landing = rill_lander::landing_of(&fx.repo, &head)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing.requested_version, x_hash);
}

#[tokio::test]
async fn conflicting_change_is_returned() {
    let fx = Fixture::new().await;
    let hooks = Arc::new(RecordingHooks::default());
    let lander = fx.lander(hooks.clone(), Arc::new(AlwaysGreen), fast_policy());
    let (base, _) = fx.commit(None, &[("f", "a\n")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();

    let (_, x) = fx.commit(Some(base), &[("f", "b\n")], "x").await;
    let (_, y) = fx.commit(Some(base), &[("f", "c\n")], "y").await;
    assert_eq!(
        lander.land_now(request(&y, RiskClass::Low)).await.unwrap(),
        LandStatus::Landed { seq: TrunkSeq(2) }
    );
    let status = lander.land_now(request(&x, RiskClass::Low)).await.unwrap();
    assert_eq!(
        status,
        LandStatus::Conflict {
            paths: vec![RepoPathBuf::new("f").unwrap()]
        }
    );
    assert_eq!(fx.head().await.0, TrunkSeq(2));
    let returned = hooks.returned.lock().await;
    assert_eq!(returned.len(), 1);
    assert_eq!(returned[0].0, x.change_id);
}

#[tokio::test]
async fn red_verifier_fails_the_request() {
    let fx = Fixture::new().await;
    let hooks = Arc::new(RecordingHooks::default());
    let verifier = Arc::new(CountingVerifier {
        calls: AtomicUsize::new(0),
        green: false,
    });
    let lander = fx.lander(hooks.clone(), verifier.clone(), fast_policy());

    let (_, trivial) = fx.commit(None, &[("t", "t")], "trivial").await;
    assert_eq!(
        lander
            .land_now(request(&trivial, RiskClass::Trivial))
            .await
            .unwrap(),
        LandStatus::Landed { seq: TrunkSeq(1) }
    );
    assert_eq!(verifier.calls.load(Ordering::SeqCst), 0);

    let (_, base) = fx.head().await;
    let (_, normal) = fx.commit(Some(base), &[("n", "n")], "normal").await;
    let status = lander
        .land_now(request(&normal, RiskClass::Normal))
        .await
        .unwrap();
    match status {
        LandStatus::Failed { reason } => assert!(reason.contains("tests failed"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(verifier.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fx.head().await.0, TrunkSeq(1));
    assert_eq!(hooks.returned.lock().await.len(), 1);
}

#[tokio::test]
async fn affected_mode_verifies_against_trunk_and_full_mode_verifies_everything() {
    let fx = Fixture::new().await;
    let seen = Arc::new(std::sync::Mutex::new(Vec::<VerifyRequest>::new()));
    let record = seen.clone();
    let verifier = Arc::new(FnVerifier(move |request: VerifyRequest| {
        record.lock().unwrap().push(request);
        async { Ok(VerificationOutcome::green("ok")) }
    }));
    let lander = fx.lander(Arc::new(NoopHooks), verifier, fast_policy());

    let (_, first) = fx.commit(None, &[("a", "a")], "first").await;
    lander
        .land_now(request(&first, RiskClass::Trivial))
        .await
        .unwrap();
    let (_, trunk) = fx.head().await;
    let trunk_root = fx.root(&trunk).await;

    let (_, normal) = fx.commit(Some(trunk), &[("n", "n")], "normal").await;
    lander
        .land_now(request(&normal, RiskClass::Normal))
        .await
        .unwrap();
    let (_, trunk) = fx.head().await;
    let (_, critical) = fx.commit(Some(trunk), &[("c", "c")], "critical").await;
    lander
        .land_now(request(&critical, RiskClass::Critical))
        .await
        .unwrap();

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        (seen[0].base, seen[0].risk, seen[0].stage),
        (
            Some(trunk_root),
            Some(RiskClass::Normal),
            VerifyStage::PreLand
        )
    );
    assert_eq!(
        (seen[1].base, seen[1].risk),
        (None, Some(RiskClass::Critical))
    );
}
