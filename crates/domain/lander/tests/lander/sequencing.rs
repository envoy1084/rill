//! The sequencer: lane order, rebasing onto the head a request will land
//! on, group commit, and ejection when approval no longer holds.

use std::sync::Arc;

use crate::common::{fast_policy, request, Fixture};
use async_trait::async_trait;
use rill_lander::{AlwaysGreen, Approval, LandStatus, NoopHooks, ReviewHooks};
use rill_types::{ChangeId, ChangeVersionHash, Lane, Principal, RiskClass, TrunkSeq};

#[tokio::test]
async fn one_sweep_lands_in_lane_order_in_one_transaction() {
    let fx = Fixture::new().await;
    let lander = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    let (base, _) = fx.commit(None, &[("base", "0")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();

    let (_, agent) = fx.commit(Some(base), &[("agent.txt", "a")], "agent").await;
    let (_, human) = fx.commit(Some(base), &[("human.txt", "h")], "human").await;
    let (_, revert) = fx
        .commit(Some(base), &[("revert.txt", "r")], "revert")
        .await;
    let agent_ticket = lander
        .enqueue(request(&agent, RiskClass::Trivial).in_lane(Lane::Agent))
        .await
        .unwrap();
    let human_ticket = lander
        .enqueue(request(&human, RiskClass::Trivial).in_lane(Lane::Human))
        .await
        .unwrap();
    let revert_ticket = lander
        .enqueue(request(&revert, RiskClass::Trivial).in_lane(Lane::Emergency))
        .await
        .unwrap();
    assert_eq!(lander.run_once().await.unwrap(), 3);

    let seq = |ticket| {
        let queue = lander.queue().clone();
        async move {
            match queue.status(ticket).await.unwrap().unwrap() {
                LandStatus::Landed { seq } => seq,
                other => panic!("{other:?}"),
            }
        }
    };
    assert_eq!(seq(revert_ticket).await, TrunkSeq(2));
    assert_eq!(seq(human_ticket).await, TrunkSeq(3));
    assert_eq!(seq(agent_ticket).await, TrunkSeq(4));
    let landed: Vec<_> = fx
        .repo
        .events_after(None, 1000)
        .await
        .unwrap()
        .into_iter()
        .filter(|(_, e)| e.kind.name() == "landed")
        .map(|(stamp, _)| stamp)
        .collect();
    assert_eq!(landed.len(), 4);
    assert!(
        landed[1..]
            .windows(2)
            .all(|w| w[0].as_bytes()[..10] == w[1].as_bytes()[..10]),
        "the three landed in one transaction"
    );
}

#[tokio::test]
async fn a_request_is_rebased_onto_the_one_landing_before_it() {
    let fx = Fixture::new().await;
    let lander = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    let body: String = (0..10).map(|i| format!("line {i}\n")).collect();
    let (base, _) = fx.commit(None, &[("f.txt", &body)], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();

    let first_body = body.replace("line 1\n", "first\n");
    let second_body = body.replace("line 8\n", "second\n");
    let (_, first) = fx
        .commit(Some(base), &[("f.txt", &first_body)], "first")
        .await;
    let (_, second) = fx
        .commit(Some(base), &[("f.txt", &second_body)], "second")
        .await;
    lander
        .enqueue(request(&first, RiskClass::Trivial))
        .await
        .unwrap();
    lander
        .enqueue(request(&second, RiskClass::Trivial))
        .await
        .unwrap();
    assert_eq!(lander.run_once().await.unwrap(), 2);

    let (seq, head) = fx.head().await;
    assert_eq!(seq, TrunkSeq(3));
    let text = fx.file(fx.root(&head).await, "f.txt").await.unwrap();
    assert!(
        text.contains("first\n") && text.contains("second\n"),
        "{text}"
    );
}

/// Approves everything except one change, as review does once its votes
/// are withdrawn.
struct Refusing {
    refused: ChangeId,
}

#[async_trait]
impl ReviewHooks for Refusing {
    async fn mark_landing(&self, _change: ChangeId) -> rill_lander::Result<()> {
        Ok(())
    }

    async fn mark_landed(
        &self,
        _change: ChangeId,
        _seq: TrunkSeq,
        _version: ChangeVersionHash,
    ) -> rill_lander::Result<()> {
        Ok(())
    }

    async fn return_to_owner(&self, _change: ChangeId, _reason: String) -> rill_lander::Result<()> {
        Ok(())
    }

    async fn dependents_of(&self, _change: ChangeId) -> rill_lander::Result<Vec<ChangeId>> {
        Ok(Vec::new())
    }

    async fn approve(
        &self,
        change: ChangeId,
        _version: ChangeVersionHash,
        _requester: &Principal,
    ) -> rill_lander::Result<Approval> {
        Ok(Approval {
            refusal: (change == self.refused).then(|| "votes were withdrawn".to_string()),
            policy: Some([9; 32]),
            ..Approval::default()
        })
    }
}

#[tokio::test]
async fn a_request_no_longer_approved_is_ejected_and_the_rest_land() {
    let fx = Fixture::new().await;
    let (base, _) = fx.commit(None, &[("base", "0")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();
    let (_, kept) = fx.commit(Some(base), &[("kept.txt", "k")], "kept").await;
    let (_, refused) = fx
        .commit(Some(base), &[("refused.txt", "r")], "refused")
        .await;
    let lander = fx.lander(
        Arc::new(Refusing {
            refused: refused.change_id,
        }),
        Arc::new(AlwaysGreen),
        fast_policy(),
    );
    let refused_ticket = lander
        .enqueue(request(&refused, RiskClass::Trivial))
        .await
        .unwrap();
    let kept_ticket = lander
        .enqueue(request(&kept, RiskClass::Trivial))
        .await
        .unwrap();
    lander.run_once().await.unwrap();

    assert_eq!(
        lander.queue().status(refused_ticket).await.unwrap(),
        Some(LandStatus::Ejected {
            reason: "votes were withdrawn".into()
        })
    );
    assert_eq!(
        lander.queue().status(kept_ticket).await.unwrap(),
        Some(LandStatus::Landed { seq: TrunkSeq(2) })
    );
    let landing = fx.repo.landing(TrunkSeq(2)).await.unwrap().unwrap();
    assert_eq!(
        landing.policy,
        Some([9; 32]),
        "the landing records what allowed it"
    );
}

#[tokio::test]
async fn a_principal_with_too_many_requests_queued_is_refused() {
    let fx = Fixture::new().await;
    let policy = rill_lander::LandPolicy {
        max_in_flight: 2,
        ..fast_policy()
    };
    let lander = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), policy);
    let (base, _) = fx.commit(None, &[("base", "0")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();
    let mut changes = Vec::new();
    for i in 0..3 {
        let name = format!("f{i}.txt");
        changes.push(fx.commit(Some(base), &[(name.as_str(), "x")], "c").await.1);
    }
    lander
        .enqueue(request(&changes[0], RiskClass::Trivial))
        .await
        .unwrap();
    let second = lander
        .enqueue(request(&changes[1], RiskClass::Trivial))
        .await
        .unwrap();
    assert_eq!(
        lander
            .enqueue(request(&changes[1], RiskClass::Trivial))
            .await
            .unwrap(),
        second,
        "asking again for a queued change is not a new request"
    );
    assert!(matches!(
        lander
            .enqueue(request(&changes[2], RiskClass::Trivial))
            .await,
        Err(rill_lander::Error::TooManyRequests { limit: 2, .. })
    ));
    lander
        .enqueue(request(&changes[2], RiskClass::Trivial).in_lane(Lane::Emergency))
        .await
        .expect("reverts are exempt");
    lander.run_once().await.unwrap();
    assert!(lander.queue().is_empty().await.unwrap());
}
