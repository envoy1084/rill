//! End-to-end review flow over in-memory stores.

use rill_review::{Error, Finding, ReviewEvent, ReviewResult};
use rill_types::objects::review::CommentCategory;
use rill_types::{Principal, RepoPathBuf, ReviewStatus, RiskClass, ThreadAnchor};

use crate::common::{harness, LIB_V0, LIB_V1};

#[tokio::test]
async fn normal_change_needs_human_plus_two_or_agents_plus_owner() {
    let h = harness().await;
    let root = h.root().await;
    let coder = Principal::agent("coder");
    let (cid, _) = h.child(&coder, root, &[("src/lib.rs", LIB_V1)]).await;
    let mut events = h.service.subscribe();

    let state = h.service.submit(&cid, &h.alice).await.unwrap();
    assert_eq!(state.status, ReviewStatus::Open);
    assert_eq!(state.risk, RiskClass::Normal);
    assert!(matches!(
        events.recv().await.unwrap(),
        ReviewEvent::Submitted { .. }
    ));

    // Agent-only approval is insufficient.
    h.service
        .vote(&cid, &h.bob, "Verified", 1, None)
        .await
        .unwrap();
    h.service
        .vote(&cid, &h.agent_a, "Review", 1, None)
        .await
        .unwrap();
    h.service
        .vote(&cid, &h.agent_b, "Review", 1, None)
        .await
        .unwrap();
    let d = h.service.evaluate(&cid).await.unwrap();
    assert!(!d.allowed, "{:?}", d.reasons);
    assert_eq!(
        h.service.get(&cid).await.unwrap().status,
        ReviewStatus::Open
    );

    // Agents cannot vote the Owner max.
    assert!(matches!(
        h.service.vote(&cid, &h.agent_a, "Owner", 1, None).await,
        Err(Error::MaxRequiresHuman(_))
    ));

    // A verification and two agent approvals plus a human owner +1 is
    // enough.
    let state = h
        .service
        .vote(&cid, &h.bob, "Owner", 1, Some("owner ok".into()))
        .await
        .unwrap();
    assert_eq!(state.status, ReviewStatus::Landable);
    let d = h.service.evaluate(&cid).await.unwrap();
    assert!(d.allowed);
    assert!(d
        .matched_policies
        .contains(&"land_normal_agent_review".to_string()));

    // Re-voting replaces rather than accumulates.
    let state = h
        .service
        .vote(&cid, &h.agent_b, "Review", -1, None)
        .await
        .unwrap();
    assert_eq!(state.current_votes().count(), 4);
    assert_eq!(state.status, ReviewStatus::Open);

    // A human +2 alone is enough.
    let state = h
        .service
        .vote(&cid, &h.alice, "Review", 2, None)
        .await
        .unwrap();
    assert_eq!(state.status, ReviewStatus::Landable);

    // A -2 veto blocks regardless.
    let state = h
        .service
        .vote(&cid, &h.bob, "Review", -2, None)
        .await
        .unwrap();
    assert_eq!(state.status, ReviewStatus::Open);
    let d = h.service.evaluate(&cid).await.unwrap();
    assert!(d
        .matched_policies
        .contains(&"block_review_veto".to_string()));
}

#[tokio::test]
async fn trivial_rebase_copies_votes_and_edit_drops_them() {
    let h = harness().await;
    let root = h.root().await;
    let (cid, v1) = h.child(&h.alice, root, &[("src/lib.rs", LIB_V1)]).await;
    h.service.submit(&cid, &h.alice).await.unwrap();
    h.service
        .vote(&cid, &h.bob, "Review", 2, None)
        .await
        .unwrap();
    h.service
        .vote(&cid, &h.agent_a, "Verified", 1, None)
        .await
        .unwrap();
    h.service
        .vote(&cid, &h.alice, "Owner", 1, None)
        .await
        .unwrap();
    assert_eq!(
        h.service.get(&cid).await.unwrap().status,
        ReviewStatus::Landable
    );

    // Message-only amend: Review and Owner copy, Verified does not.
    let v2 = h
        .amend(cid, v1, root, &[("src/lib.rs", LIB_V1)], "better message")
        .await;
    let state = h.service.add_version(&cid, v2).await.unwrap();
    assert_eq!(state.versions, vec![v1, v2]);
    let mut labels: Vec<_> = state.current_votes().map(|v| v.label.as_str()).collect();
    labels.sort_unstable();
    assert_eq!(labels, vec!["Owner", "Review"]);
    assert_eq!(state.status, ReviewStatus::Landable);

    // Trivial rebase onto a trunk that moved elsewhere: still copied.
    let (_, root2) = h.child(&h.bob, root, &[("docs/x.md", "x\n")]).await;
    let v3 = h
        .amend(cid, v2, root2, &[("src/lib.rs", LIB_V1)], "better message")
        .await;
    let state = h.service.add_version(&cid, v3).await.unwrap();
    assert_eq!(state.current_votes().count(), 2);
    assert_eq!(state.status, ReviewStatus::Landable);

    // A real edit drops every vote and the change goes back to Open.
    let edited = LIB_V1.replace("a - b", "a - b - 0");
    let v4 = h
        .amend(cid, v3, root2, &[("src/lib.rs", &edited)], "better message")
        .await;
    let state = h.service.add_version(&cid, v4).await.unwrap();
    assert_eq!(state.current_votes().count(), 0);
    assert_eq!(state.votes.len(), 7, "old votes stay in history");
    assert_eq!(state.status, ReviewStatus::Open);
}

#[tokio::test]
async fn threads_reanchor_and_block_landing() {
    let h = harness().await;
    let root = h.root().await;
    let (cid, v1) = h.child(&h.alice, root, &[("src/lib.rs", LIB_V1)]).await;
    h.service.submit(&cid, &h.alice).await.unwrap();
    let path = RepoPathBuf::new("src/lib.rs").unwrap();

    // Comment on the `a - b` line (line 6 of LIB_V1).
    let tid = h
        .service
        .comment(
            &cid,
            &h.agent_a,
            ThreadAnchor::Lines {
                version: v1,
                path: path.clone(),
                start: 6,
                end: 6,
                context_hash: String::new(),
            },
            "overflow is unchecked",
            Some(CommentCategory::Bug),
            Some(80),
            Some(h.alice.clone()),
        )
        .await
        .unwrap();
    let state = h.service.get(&cid).await.unwrap();
    let ThreadAnchor::Lines { context_hash, .. } = &state.threads[0].anchor else {
        panic!("expected a line anchor");
    };
    assert!(!context_hash.is_empty());

    // An unresolved Bug blocks even a fully approved change.
    h.service
        .vote(&cid, &h.bob, "Review", 2, None)
        .await
        .unwrap();
    let d = h.service.evaluate(&cid).await.unwrap();
    assert!(!d.allowed);
    assert!(d
        .matched_policies
        .contains(&"block_unresolved_bug_or_security".to_string()));

    // New version inserts lines above; the anchor follows.
    let shifted = format!("//! crate docs\n//! more docs\n{LIB_V1}");
    let v2 = h
        .amend(cid, v1, root, &[("src/lib.rs", &shifted)], "child")
        .await;
    let state = h.service.add_version(&cid, v2).await.unwrap();
    match &state.threads[0].anchor {
        ThreadAnchor::Lines {
            version,
            start,
            end,
            ..
        } => {
            assert_eq!(*version, v2);
            assert_eq!((*start, *end), (8, 8));
        }
        other => panic!("unexpected anchor {other:?}"),
    }

    // Resolving the thread and re-approving makes it landable.
    h.service
        .vote(&cid, &h.bob, "Review", 2, None)
        .await
        .unwrap();
    let state = h.service.resolve(&cid, &tid, &h.alice).await.unwrap();
    assert_eq!(state.status, ReviewStatus::Landable);
}

#[tokio::test]
async fn stacks_validate_and_restack() {
    let h = harness().await;
    let root = h.root().await;
    let (c1, v1) = h.child(&h.alice, root, &[("a.rs", "1\n")]).await;
    let (c2, _) = h.child(&h.alice, v1, &[("b.rs", "2\n")]).await;
    let (c3, _) = h.child(&h.alice, root, &[("c.rs", "3\n")]).await;
    for c in [c1, c2, c3] {
        h.service.submit(&c, &h.alice).await.unwrap();
    }
    assert!(matches!(
        h.service.set_stack(&[c1, c3]).await,
        Err(Error::InvalidStack(_))
    ));
    let states = h.service.set_stack(&[c1, c2]).await.unwrap();
    assert_eq!(states[1].stack, vec![c1, c2]);
    let facts = h.service.change_facts(&states[1]).await.unwrap();
    assert_eq!(facts.stack_depth, 1);
    let restack = h
        .service
        .restack_after_land(&c1, v1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(restack.dependents, vec![c2]);
    assert_eq!(restack.base, v1);
}

#[tokio::test]
async fn a_reviewer_result_lands_on_the_version_it_reviewed_only() {
    let h = harness().await;
    let root = h.root().await;
    let (cid, v1) = h.child(&h.agent_a, root, &[("src/lib.rs", LIB_V1)]).await;
    h.service.submit(&cid, &h.alice).await.unwrap();
    let reviewer = Principal::agent("reviewer");
    let result = ReviewResult {
        version: v1,
        findings: vec![
            Finding::at(
                RepoPathBuf::new("src/lib.rs").unwrap(),
                1,
                1,
                CommentCategory::Bug,
                90,
                "subtracts instead of adding",
            ),
            Finding::change(CommentCategory::Suggestion, 10, "too unsure to post"),
        ],
        vote: -1,
        summary: "one bug".into(),
    };
    let posted = h
        .service
        .post_review_result(&cid, &reviewer, &result)
        .await
        .unwrap();
    assert_eq!((posted.threads.len(), posted.vote), (1, -1));
    let state = h.service.get(&cid).await.unwrap();
    let vote = state.current_votes().next().unwrap();
    assert_eq!((&vote.voter, vote.value), (&reviewer, -1));
    assert!(!h.service.evaluate(&cid).await.unwrap().allowed);

    let edited = LIB_V1.replace("a - b", "a + b");
    let v2 = h
        .amend(cid, v1, root, &[("src/lib.rs", &edited)], "fix")
        .await;
    h.service.add_version(&cid, v2).await.unwrap();
    let late = ReviewResult {
        vote: 1,
        findings: vec![],
        ..result
    };
    assert!(matches!(
        h.service.post_review_result(&cid, &reviewer, &late).await,
        Err(Error::StaleVersion { .. })
    ));
    assert_eq!(
        h.service.get(&cid).await.unwrap().current_votes().count(),
        0
    );
}

#[tokio::test]
async fn review_context_carries_blame_and_intent() {
    let h = harness().await;
    let root = h.root().await;
    let changed = LIB_V0.replace("a + b", "a.wrapping_add(b)");
    let (cid, v) = h.child(&h.bob, root, &[("src/lib.rs", &changed)]).await;
    h.service.submit(&cid, &h.bob).await.unwrap();
    let ctx = h
        .service
        .review_context(&cid, v, rill_review::Budget::default())
        .await
        .unwrap();
    assert_eq!(ctx.diffs.len(), 1);
    assert_eq!(ctx.diffs[0].hunks.len(), 1);
    assert_eq!(ctx.blame_for_touched_lines.len(), 1);
    assert_eq!(ctx.blame_for_touched_lines[0].line_no, 2);
    assert_eq!(ctx.blame_for_touched_lines[0].blame.author, h.alice);
    assert_eq!(ctx.stack, vec![cid]);
    assert!(!ctx.truncated);
}
