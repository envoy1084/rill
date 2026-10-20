//! End-to-end review flow over in-memory stores.

use std::sync::Arc;

use rill_core::{ChangeBuilder, CommitGraph, DeriveManager, TreeOps};
use rill_policy::{RiskConfig, SubmitPolicy};
use rill_review::agents::{HeuristicReviewer, PoolConfig, ReviewerPool};
use rill_review::{Error, LabelSet, ReviewEvent, ReviewService};
use rill_store::{RepoStore, Stores};
use rill_types::objects::review::CommentCategory;
use rill_types::{
    ChangeId, ChangeVersionHash, Principal, RepoPathBuf, ReviewStatus, RiskClass, ThreadAnchor,
};

struct Harness {
    repo: RepoStore,
    service: ReviewService,
    alice: Principal,
    bob: Principal,
    agent_a: Principal,
    agent_b: Principal,
}

async fn harness() -> Harness {
    let stores = Stores::memory();
    let config = stores.create_repo("flow", "").await.unwrap();
    let repo = stores.repo(config.id);
    let trees = TreeOps::new(repo.objects().clone(), 64);
    let derive = DeriveManager::new(repo.clone(), trees, 64);
    let graph = Arc::new(CommitGraph::new(repo.clone(), derive.clone()));
    let service = ReviewService::new(
        repo.clone(),
        derive,
        graph,
        Arc::new(SubmitPolicy::default_rules()),
        Arc::new(RiskConfig::default().compile().unwrap()),
        Arc::new(LabelSet::default()),
    );
    Harness {
        repo,
        service,
        alice: Principal::human("alice"),
        bob: Principal::human("bob"),
        agent_a: Principal::agent("reviewer-a"),
        agent_b: Principal::agent("reviewer-b"),
    }
}

const LIB_V0: &str = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
const LIB_V1: &str =
    "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn sub(a: i32, b: i32) -> i32 {\n    a - b\n}\n";

impl Harness {
    async fn root(&self) -> ChangeVersionHash {
        ChangeBuilder::new(self.alice.clone())
            .message("root")
            .file("src/lib.rs", LIB_V0)
            .unwrap()
            .file("README.md", "# demo\n")
            .unwrap()
            .commit(&self.repo)
            .await
            .unwrap()
            .0
    }

    async fn child(
        &self,
        author: &Principal,
        parent: ChangeVersionHash,
        files: &[(&str, &str)],
    ) -> (ChangeId, ChangeVersionHash) {
        let mut b = ChangeBuilder::new(author.clone())
            .parent(parent)
            .message("child");
        for (p, c) in files {
            b = b.file(p, c.to_string()).unwrap();
        }
        let (h, cv) = b.commit(&self.repo).await.unwrap();
        (cv.change_id, h)
    }

    async fn amend(
        &self,
        change_id: ChangeId,
        predecessor: ChangeVersionHash,
        parent: ChangeVersionHash,
        files: &[(&str, &str)],
        message: &str,
    ) -> ChangeVersionHash {
        let mut b = ChangeBuilder::new(self.alice.clone())
            .change_id(change_id)
            .parent(parent)
            .predecessor(predecessor)
            .message(message);
        for (p, c) in files {
            b = b.file(p, c.to_string()).unwrap();
        }
        b.commit(&self.repo).await.unwrap().0
    }
}

#[tokio::test]
async fn normal_change_needs_human_plus_two_or_agents_plus_owner() {
    let h = harness().await;
    let root = h.root().await;
    let (cid, _) = h.child(&h.agent_a, root, &[("src/lib.rs", LIB_V1)]).await;
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

    // Two agent approvals plus a human owner +1 is enough.
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
        .contains(&"land_normal_agent_review_with_owner".to_string()));

    // Re-voting replaces rather than accumulates.
    let state = h
        .service
        .vote(&cid, &h.agent_b, "Review", -1, None)
        .await
        .unwrap();
    assert_eq!(state.current_votes().count(), 3);
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
    let labels: Vec<_> = state.current_votes().map(|v| v.label.as_str()).collect();
    assert_eq!(labels, vec!["Review", "Owner"]);
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
async fn heuristic_reviewer_posts_findings_and_votes() {
    let h = harness().await;
    let root = h.root().await;
    let leaky = format!("{LIB_V1}\nconst AWS_KEY: &str = \"AKIAABCDEFGHIJKLMNOP\";\n// TODO remove\npub fn dbg() {{ println!(\"here\"); }}\n");
    let (cid, v1) = h.child(&h.agent_a, root, &[("src/lib.rs", &leaky)]).await;
    h.service.submit(&cid, &h.alice).await.unwrap();

    let pool = ReviewerPool::new(PoolConfig::default()).with(HeuristicReviewer::default().shared());
    let report = pool.run(&h.service, &cid).await.unwrap();
    assert_eq!(report.version, v1);
    let outcome = &report.outcomes[0];
    assert_eq!(outcome.name, "heuristics");
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let categories: Vec<_> = outcome.findings.iter().map(|f| f.category).collect();
    assert!(categories.contains(&CommentCategory::Security));
    assert!(categories.contains(&CommentCategory::Suggestion));
    assert!(categories.contains(&CommentCategory::Style));
    assert!(categories.contains(&CommentCategory::TestGap));
    assert_eq!(outcome.vote, Some(-1));

    let state = h.service.get(&cid).await.unwrap();
    assert_eq!(state.threads.len(), outcome.thread_ids.len());
    let vote = state.current_votes().next().unwrap();
    assert_eq!(vote.voter.name, "heuristics");
    assert_eq!(vote.value, -1);
    let d = h.service.evaluate(&cid).await.unwrap();
    assert!(!d.allowed);

    // A clean change gets +1 and only the test-gap note.
    let (clean, _) = h
        .child(
            &h.alice,
            root,
            &[
                ("src/lib.rs", LIB_V1),
                ("tests/add.rs", "#[test] fn t() {}\n"),
            ],
        )
        .await;
    h.service.submit(&clean, &h.alice).await.unwrap();
    let report = pool.run(&h.service, &clean).await.unwrap();
    assert_eq!(report.outcomes[0].vote, Some(1));
    assert!(report.outcomes[0].findings.is_empty());
}

#[tokio::test]
async fn review_context_carries_blame_and_intent() {
    let h = harness().await;
    let root = h.root().await;
    let changed = LIB_V0.replace("a + b", "a.wrapping_add(b)");
    let (cid, _) = h.child(&h.bob, root, &[("src/lib.rs", &changed)]).await;
    h.service.submit(&cid, &h.bob).await.unwrap();
    let ctx = h
        .service
        .review_context(&cid, rill_review::Budget::default())
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
