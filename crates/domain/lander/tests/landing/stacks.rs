//! Stacks as ordered groups: they land whole or as a prefix, each member
//! on the one below it, and what stays stacked above a landing is
//! restacked.

use std::sync::Arc;

use rill_lander::{AlwaysGreen, LandStatus, Lander};
use rill_types::{ChangeVersion, ChangeVersionHash, GroupMode, LandTicket, RiskClass, TrunkSeq};

use crate::common::{fast_policy, request, Fixture};
use crate::doubles::StackHooks;

/// Trunk with `b` at "1", and a stack a ← b ← c on it, where b edits `b`.
async fn stack(fx: &Fixture) -> (ChangeVersionHash, [ChangeVersion; 3]) {
    let (base, _) = fx.commit(None, &[("b", "1\n")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();
    let (a, a_cv) = fx.commit(Some(base), &[("a", "a\n")], "a").await;
    let (b, b_cv) = fx.commit(Some(a), &[("b", "2\n")], "b").await;
    let (_, c_cv) = fx.commit(Some(b), &[("c", "c\n")], "c").await;
    (base, [a_cv, b_cv, c_cv])
}

/// Trunk moves `b` to "3", so the stack's middle change conflicts.
async fn conflict_on_trunk(fx: &Fixture, base: ChangeVersionHash) {
    let (moved, _) = fx.commit(Some(base), &[("b", "3\n")], "elsewhere").await;
    fx.repo.land_on_trunk(Some(base), moved).await.unwrap();
}

async fn land(
    fx: &Fixture,
    members: &[ChangeVersion],
    mode: GroupMode,
) -> (Arc<Lander>, Arc<StackHooks>, Vec<LandStatus>) {
    let hooks = Arc::new(StackHooks::new(
        members.iter().map(|c| c.change_id).collect(),
    ));
    let lander = fx.lander(hooks.clone(), Arc::new(AlwaysGreen), fast_policy());
    let requests = members
        .iter()
        .map(|c| request(c, RiskClass::Normal))
        .collect();
    let tickets = lander.enqueue_group(requests, mode).await.unwrap();
    lander.run_once().await.unwrap();
    let statuses = statuses(&lander, &tickets).await;
    (lander, hooks, statuses)
}

async fn statuses(lander: &Lander, tickets: &[LandTicket]) -> Vec<LandStatus> {
    let mut out = Vec::new();
    for t in tickets {
        out.push(lander.queue().status(*t).await.unwrap().unwrap());
    }
    out
}

fn landed(status: &LandStatus) -> bool {
    matches!(status, LandStatus::Landed { .. })
}

#[tokio::test]
async fn a_stack_lands_whole_in_order() {
    let fx = Fixture::new().await;
    let (_, members) = stack(&fx).await;
    let (_, _, statuses) = land(&fx, &members, GroupMode::All).await;
    assert_eq!(
        statuses,
        (2..=4)
            .map(|seq| LandStatus::Landed { seq: TrunkSeq(seq) })
            .collect::<Vec<_>>()
    );
    let (_, head) = fx.head().await;
    let root = fx.root(&head).await;
    for (path, body) in [("a", "a\n"), ("b", "2\n"), ("c", "c\n")] {
        assert_eq!(fx.file(root, path).await.as_deref(), Some(body));
    }
}

#[tokio::test]
async fn when_one_member_cannot_land_none_of_the_stack_does() {
    let fx = Fixture::new().await;
    let (base, members) = stack(&fx).await;
    conflict_on_trunk(&fx, base).await;
    let (_, hooks, statuses) = land(&fx, &members, GroupMode::All).await;
    assert!(statuses.iter().all(|s| !landed(s)), "{statuses:?}");
    assert!(matches!(statuses[1], LandStatus::Conflict { .. }));
    assert_eq!(fx.head().await.0, TrunkSeq(2), "trunk did not move");
    assert_eq!(hooks.returned.lock().await.len(), 3, "every owner hears");
}

#[tokio::test]
async fn a_prefix_lands_up_to_the_first_member_that_cannot() {
    let fx = Fixture::new().await;
    let (base, members) = stack(&fx).await;
    conflict_on_trunk(&fx, base).await;
    let (_, hooks, statuses) = land(&fx, &members, GroupMode::Prefix).await;
    assert_eq!(statuses[0], LandStatus::Landed { seq: TrunkSeq(3) });
    assert!(matches!(statuses[1], LandStatus::Conflict { .. }));
    assert!(matches!(&statuses[2], LandStatus::Failed { reason } if reason.contains("below it")));
    let (_, head) = fx.head().await;
    assert_eq!(
        *hooks.restacked.lock().await,
        vec![(members[0].change_id, head)],
        "what stays above the landed prefix is restacked onto it"
    );
}

#[tokio::test]
async fn landing_the_bottom_alone_restacks_the_rest_and_a_whole_stack_does_not() {
    let fx = Fixture::new().await;
    let (_, members) = stack(&fx).await;
    let hooks = Arc::new(StackHooks::new(
        members.iter().map(|c| c.change_id).collect(),
    ));
    let lander = fx.lander(hooks.clone(), Arc::new(AlwaysGreen), fast_policy());
    lander
        .land_now(request(&members[0], RiskClass::Normal))
        .await
        .unwrap();
    let (_, head) = fx.head().await;
    assert_eq!(
        *hooks.restacked.lock().await,
        vec![(members[0].change_id, head)]
    );

    let fx = Fixture::new().await;
    let (_, members) = stack(&fx).await;
    let (_, hooks, _) = land(&fx, &members, GroupMode::All).await;
    assert!(hooks.restacked.lock().await.is_empty());
}

#[tokio::test]
async fn a_member_already_queued_refuses_the_group() {
    let fx = Fixture::new().await;
    let (_, members) = stack(&fx).await;
    let lander = fx.lander(
        Arc::new(StackHooks::new(vec![])),
        Arc::new(AlwaysGreen),
        fast_policy(),
    );
    lander
        .enqueue(request(&members[1], RiskClass::Normal))
        .await
        .unwrap();
    let requests = members
        .iter()
        .map(|c| request(c, RiskClass::Normal))
        .collect();
    assert!(matches!(
        lander.enqueue_group(requests, GroupMode::All).await,
        Err(rill_lander::Error::AlreadyQueued { .. })
    ));
    assert_eq!(
        lander.queue().pending(10).await.unwrap().len(),
        1,
        "nothing else queued"
    );
}
