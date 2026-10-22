//! A self-healing trunk: culprits open incidents, a revert or forward fix
//! moves them to fixing and verification closes them; a red trunk finds a
//! second break on its own; guarded and frozen trunks hold landings back.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rill_lander::{revert, LandPolicy, LandStatus, Lander, PostLandVerifier};
use rill_types::{
    HealthState, IncidentState, Lane, Principal, Resolution, RiskClass, TrunkHealth, TrunkSeq,
};

use crate::common::{fast_policy, request, Fixture};
use crate::doubles::{RecordingHooks, StepsVerifier};

fn steps(fx: &Fixture) -> Arc<StepsVerifier> {
    Arc::new(StepsVerifier {
        trees: fx.trees.clone(),
        steps: vec!["a", "b"],
        calls: AtomicUsize::new(0),
    })
}

/// Land `files` one change each, in order, on trunk's head.
async fn land(fx: &Fixture, lander: &Arc<Lander>, files: &[(&str, &str)]) {
    for (path, body) in files {
        let parent = fx.repo.trunk_head().await.unwrap().map(|(_, h)| h);
        let (_, cv) = fx.commit(parent, &[(path, body)], path).await;
        let status = lander
            .land_now(request(&cv, RiskClass::Trivial))
            .await
            .unwrap();
        assert!(matches!(status, LandStatus::Landed { .. }), "{status:?}");
    }
}

/// Sweep until the worker has verified the head.
async fn catch_up(worker: &Arc<PostLandVerifier>) {
    while worker.run_once().await.unwrap().is_some() {}
}

fn no_auto_revert() -> LandPolicy {
    LandPolicy {
        auto_revert: false,
        ..fast_policy()
    }
}

#[tokio::test]
async fn a_culprit_opens_an_incident_its_revert_fixes_and_verification_closes() {
    let fx = Fixture::new().await;
    let hooks = Arc::new(RecordingHooks::default());
    let lander = fx.lander(hooks.clone(), steps(&fx), no_auto_revert());
    let worker = PostLandVerifier::new(lander.clone(), 8);
    land(
        &fx,
        &lander,
        &[("one", "1"), ("bad_a", "!"), ("three", "3")],
    )
    .await;
    catch_up(&worker).await;

    let incident = fx.repo.incident(TrunkSeq(2)).await.unwrap().unwrap();
    assert_eq!(incident.state, IncidentState::Open);
    assert_eq!(incident.steps, vec!["a"]);
    assert_eq!(incident.failures, vec!["step:a"]);
    let health = fx.repo.trunk_health().await.unwrap();
    assert_eq!(
        (health.state, health.open.clone()),
        (HealthState::Red, vec![TrunkSeq(2)])
    );
    assert_eq!(hooks.incidents.lock().await.len(), 1, "the owner heard");

    let reverted = revert(&lander, TrunkSeq(2), "breaks a", Principal::human("ann"))
        .await
        .unwrap();
    let incident = fx.repo.incident(TrunkSeq(2)).await.unwrap().unwrap();
    assert_eq!(incident.state, IncidentState::Fixing);
    assert_eq!(
        incident.resolution,
        Some(Resolution::Reverted(reverted.seq))
    );
    assert_eq!(
        fx.repo.trunk_health().await.unwrap().state,
        HealthState::Fixing
    );

    catch_up(&worker).await;
    let incident = fx.repo.incident(TrunkSeq(2)).await.unwrap().unwrap();
    assert_eq!(incident.state, IncidentState::Closed);
    assert_eq!(
        fx.repo.trunk_health().await.unwrap(),
        TrunkHealth {
            since: fx.repo.trunk_health().await.unwrap().since,
            ..TrunkHealth::default()
        }
    );
}

#[tokio::test]
async fn a_fixes_trailer_cancels_the_automatic_revert_and_a_mention_does_not() {
    let fx = Fixture::new().await;
    let lander = fx.lander(
        Arc::new(RecordingHooks::default()),
        steps(&fx),
        no_auto_revert(),
    );
    land(
        &fx,
        &lander,
        &[("bad_a", "!"), ("mention", "after trunk@1 broke")],
    )
    .await;
    assert!(!rill_lander::has_forward_fix(&lander, TrunkSeq(1))
        .await
        .unwrap());
    let (_, head) = fx.head().await;
    let (_, fix) = fx.delete(head, "bad_a", "Repair a\n\nFixes: trunk@1").await;
    lander
        .land_now(request(&fix, RiskClass::Trivial))
        .await
        .unwrap();
    assert!(rill_lander::has_forward_fix(&lander, TrunkSeq(1))
        .await
        .unwrap());
    assert!(rill_lander::auto_revert(&lander, TrunkSeq(1))
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn a_second_break_on_a_red_trunk_is_found_and_gets_its_own_incident() {
    let fx = Fixture::new().await;
    let verifier = steps(&fx);
    let lander = fx.lander(
        Arc::new(RecordingHooks::default()),
        verifier.clone(),
        no_auto_revert(),
    );
    let worker = PostLandVerifier::new(lander.clone(), 8);
    land(&fx, &lander, &[("bad_a", "!")]).await;
    catch_up(&worker).await;
    land(&fx, &lander, &[("x", "1"), ("bad_b", "!"), ("y", "2")]).await;
    catch_up(&worker).await;

    let open: Vec<_> = lander
        .open_incidents()
        .await
        .unwrap()
        .into_iter()
        .map(|i| (i.culprit, i.steps))
        .collect();
    assert_eq!(
        open,
        vec![
            (TrunkSeq(3), vec!["b".to_string()]),
            (TrunkSeq(1), vec!["a".to_string()])
        ]
    );
    assert_eq!(
        fx.repo.trunk_health().await.unwrap().failing_steps,
        vec!["a", "b"]
    );
}

#[tokio::test]
async fn only_landings_that_could_affect_the_failing_steps_are_searched() {
    let fx = Fixture::new().await;
    let verifier = steps(&fx);
    let lander = fx.lander(
        Arc::new(RecordingHooks::default()),
        verifier.clone(),
        no_auto_revert(),
    );
    let worker = PostLandVerifier::with_full_every(lander.clone(), 32, Duration::from_secs(3600));
    land(&fx, &lander, &[("zero", "0")]).await;
    catch_up(&worker).await;
    let files: Vec<(String, &str)> = (0..12)
        .map(|i| {
            (
                if i == 7 {
                    "bad_a".into()
                } else {
                    format!("f{i}")
                },
                "x",
            )
        })
        .collect();
    let files: Vec<(&str, &str)> = files.iter().map(|(p, b)| (p.as_str(), *b)).collect();
    land(&fx, &lander, &files).await;
    verifier.calls.store(0, Ordering::SeqCst);
    catch_up(&worker).await;
    assert!(fx.repo.incident(TrunkSeq(9)).await.unwrap().is_some());
    assert!(
        verifier.calls.load(Ordering::SeqCst) <= 6,
        "one candidate, so barely any probes: {} verifications",
        verifier.calls.load(Ordering::SeqCst)
    );
}

#[tokio::test]
async fn a_guarded_trunk_holds_what_touches_the_failing_steps_and_a_frozen_one_all_but_emergencies()
{
    let fx = Fixture::new().await;
    let lander = fx.lander(
        Arc::new(RecordingHooks::default()),
        steps(&fx),
        no_auto_revert(),
    );
    // Trunk is broken: step `a` fails while `bad_a` exists.
    let (base, _) = fx.commit(None, &[("bad_a", "!")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();
    let guarded = TrunkHealth {
        state: HealthState::Guarded,
        open: vec![TrunkSeq(9)],
        failing_steps: vec!["a".into()],
        ..TrunkHealth::default()
    };
    fx.repo
        .set_trunk_health(&guarded, &Principal::human("ann"))
        .await
        .unwrap();
    let (_, head) = fx.head().await;
    let (_, touches) = fx
        .commit(Some(head), &[("bad_a", "worse")], "touches a")
        .await;
    let (_, elsewhere) = fx.commit(Some(head), &[("docs", "d")], "elsewhere").await;
    let (_, fix) = fx.delete(head, "bad_a", "repair\n\nFixes: trunk@9").await;
    let held = lander
        .enqueue(request(&touches, RiskClass::Normal))
        .await
        .unwrap();
    for cv in [&elsewhere, &fix] {
        lander
            .enqueue(request(cv, RiskClass::Normal))
            .await
            .unwrap();
    }
    lander.run_once().await.unwrap();
    assert_eq!(
        lander.queue().status(held).await.unwrap(),
        Some(LandStatus::Queued),
        "it waits for trunk to heal"
    );
    assert_eq!(fx.head().await.0, TrunkSeq(3), "the other two landed");

    lander
        .set_frozen(true, Some("incident".into()), &Principal::human("ann"))
        .await
        .unwrap();
    let (_, head) = fx.head().await;
    let (_, normal) = fx.commit(Some(head), &[("more", "1")], "more").await;
    let (_, urgent) = fx.commit(Some(head), &[("hotfix", "1")], "hotfix").await;
    let normal = lander
        .enqueue(request(&normal, RiskClass::Normal))
        .await
        .unwrap();
    lander
        .enqueue(request(&urgent, RiskClass::Normal).in_lane(Lane::Emergency))
        .await
        .unwrap();
    lander.run_once().await.unwrap();
    assert_eq!(
        lander.queue().status(normal).await.unwrap(),
        Some(LandStatus::Queued)
    );
    assert_eq!(fx.head().await.0, TrunkSeq(4), "only the emergency landed");
}
