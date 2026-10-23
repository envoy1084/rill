//! Sequencer v1: independent requests are speculated in parallel and lined
//! up at commit; a failure holds back only what overlaps it; configuration
//! lands alone; swarms are throttled and limited.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::common::{fast_policy, request, Fixture};
use async_trait::async_trait;
use rill_lander::{
    LandEvent, LandPolicy, LandStatus, Lander, NoopHooks, VerificationOutcome, Verifier,
    VerifyRequest,
};
use rill_types::{ChangeVersionHash, LandTicket, Principal, RiskClass, TrunkSeq};

/// Green after a pause, counting how many verifications overlap.
#[derive(Default)]
struct Concurrent {
    now: AtomicUsize,
    most: AtomicUsize,
}

#[async_trait]
impl Verifier for Concurrent {
    async fn verify(&self, _request: VerifyRequest) -> rill_lander::Result<VerificationOutcome> {
        let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.now.fetch_sub(1, Ordering::SeqCst);
        Ok(VerificationOutcome::green(""))
    }
}

async fn status(lander: &Lander, ticket: LandTicket) -> LandStatus {
    lander.queue().status(ticket).await.unwrap().unwrap()
}

async fn base(fx: &Fixture, files: &[(&str, &str)]) -> ChangeVersionHash {
    let (base, _) = fx.commit(None, files, "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();
    base
}

fn lines(n: usize) -> String {
    (0..n).map(|i| format!("line {i}\n")).collect()
}

#[tokio::test]
async fn independent_requests_are_verified_in_parallel_and_land_as_one_line() {
    let fx = Fixture::new().await;
    let verifier = Arc::new(Concurrent::default());
    let lander = fx.lander(Arc::new(NoopHooks), verifier.clone(), fast_policy());
    let base = base(&fx, &[("base", "0")]).await;
    let mut tickets = Vec::new();
    for name in ["a", "b", "c", "d"] {
        let (_, cv) = fx.commit(Some(base), &[(name, name)], name).await;
        tickets.push(
            lander
                .enqueue(request(&cv, RiskClass::Normal))
                .await
                .unwrap(),
        );
    }
    lander.run_once().await.unwrap();
    let mut seqs = Vec::new();
    for t in &tickets {
        match status(&lander, *t).await {
            LandStatus::Landed { seq } => seqs.push(seq.get()),
            other => panic!("{other:?}"),
        }
    }
    seqs.sort_unstable();
    assert_eq!(seqs, vec![2, 3, 4, 5]);
    assert!(
        verifier.most.load(Ordering::SeqCst) >= 2,
        "speculated in parallel"
    );
    let (_, head) = fx.head().await;
    let root = fx.root(&head).await;
    for name in ["a", "b", "c", "d"] {
        assert_eq!(fx.file(root, name).await.as_deref(), Some(name));
    }
    for seq in 2..=5u64 {
        let at = fx.repo.trunk_at(TrunkSeq(seq)).await.unwrap().unwrap();
        let before = fx.repo.trunk_at(TrunkSeq(seq - 1)).await.unwrap().unwrap();
        let cv = fx.repo.get_change_version(&at).await.unwrap().unwrap();
        assert_eq!(cv.first_parent(), Some(before), "trunk is one line");
    }
}

#[tokio::test]
async fn a_conflict_holds_back_nothing_independent_and_its_dependents_go_on_without_it() {
    let fx = Fixture::new().await;
    let lander = fx.lander(
        Arc::new(NoopHooks),
        Arc::new(rill_lander::AlwaysGreen),
        fast_policy(),
    );
    let body = lines(10);
    let base = base(&fx, &[("f.txt", &body), ("g.txt", "g\n")]).await;
    let edit = |from: &str, to: &str| body.replace(from, to);
    let (_, a) = fx
        .commit(Some(base), &[("f.txt", &edit("line 1\n", "a\n"))], "a")
        .await;
    let (_, b) = fx
        .commit(Some(base), &[("f.txt", &edit("line 1\n", "b\n"))], "b")
        .await;
    let (_, c) = fx.commit(Some(base), &[("g.txt", "c\n")], "c").await;
    let (_, d) = fx
        .commit(Some(base), &[("f.txt", &edit("line 8\n", "d\n"))], "d")
        .await;
    let mut tickets = Vec::new();
    for cv in [&a, &b, &c, &d] {
        tickets.push(
            lander
                .enqueue(request(cv, RiskClass::Trivial))
                .await
                .unwrap(),
        );
        // Queue order is by the millisecond; keep these in order.
        tokio::time::sleep(Duration::from_millis(3)).await;
    }
    lander.run_once().await.unwrap();
    assert!(matches!(
        status(&lander, tickets[1]).await,
        LandStatus::Conflict { .. }
    ));
    for t in [tickets[0], tickets[2], tickets[3]] {
        assert!(
            matches!(status(&lander, t).await, LandStatus::Landed { .. }),
            "{t:?}"
        );
    }
    let (seq, head) = fx.head().await;
    assert_eq!(seq, TrunkSeq(4));
    let root = fx.root(&head).await;
    let f = fx.file(root, "f.txt").await.unwrap();
    assert!(
        f.contains("a\n") && f.contains("d\n") && !f.contains("b\n"),
        "{f}"
    );
    assert_eq!(fx.file(root, "g.txt").await.as_deref(), Some("c\n"));
}

#[tokio::test]
async fn a_request_overlapping_two_chains_lands_on_both() {
    let fx = Fixture::new().await;
    let lander = fx.lander(
        Arc::new(NoopHooks),
        Arc::new(rill_lander::AlwaysGreen),
        fast_policy(),
    );
    let (x, y) = (lines(10), lines(10));
    let base = base(&fx, &[("x", &x), ("y", &y)]).await;
    let (_, a) = fx
        .commit(Some(base), &[("x", &x.replace("line 1\n", "a\n"))], "a")
        .await;
    let (_, b) = fx
        .commit(Some(base), &[("y", &y.replace("line 1\n", "b\n"))], "b")
        .await;
    let (_, c) = fx
        .commit(
            Some(base),
            &[
                ("x", &x.replace("line 8\n", "c\n")),
                ("y", &y.replace("line 8\n", "c\n")),
            ],
            "c",
        )
        .await;
    for cv in [&a, &b, &c] {
        lander
            .enqueue(request(cv, RiskClass::Trivial))
            .await
            .unwrap();
    }
    lander.run_once().await.unwrap();
    let (seq, head) = fx.head().await;
    assert_eq!(seq, TrunkSeq(4));
    let root = fx.root(&head).await;
    let (x, y) = (
        fx.file(root, "x").await.unwrap(),
        fx.file(root, "y").await.unwrap(),
    );
    assert!(x.contains("a\n") && x.contains("c\n"), "{x}");
    assert!(y.contains("b\n") && y.contains("c\n"), "{y}");
}

#[tokio::test]
async fn a_configuration_change_lands_in_a_batch_of_its_own() {
    let fx = Fixture::new().await;
    let lander = fx.lander(
        Arc::new(NoopHooks),
        Arc::new(rill_lander::AlwaysGreen),
        fast_policy(),
    );
    let base = base(&fx, &[("base", "0")]).await;
    let (_, a) = fx.commit(Some(base), &[("a", "a")], "a").await;
    let (_, rules) = fx
        .commit(
            Some(base),
            &[(".rill/landing.toml", "[throttle]\nmin_attempts = 3\n")],
            "rules",
        )
        .await;
    let (_, c) = fx.commit(Some(base), &[("c", "c")], "c").await;
    let mut tickets = Vec::new();
    for cv in [&a, &rules, &c] {
        tickets.push(
            lander
                .enqueue(request(cv, RiskClass::Trivial))
                .await
                .unwrap(),
        );
    }
    lander.run_once().await.unwrap();
    assert_eq!(fx.head().await.0, TrunkSeq(4));
    let LandStatus::Landed { seq } = status(&lander, tickets[1]).await else {
        panic!("the rules did not land");
    };
    let stamps: Vec<_> = fx
        .repo
        .events_after(None, 1000)
        .await
        .unwrap()
        .into_iter()
        .filter(|(_, e)| e.kind.name() == "landed")
        .map(|(stamp, _)| stamp.as_bytes()[..10].to_vec())
        .collect();
    let own = &stamps[seq.get() as usize - 1];
    assert_eq!(
        stamps[1..].iter().filter(|s| *s == own).count(),
        1,
        "the rules landed in a transaction of their own"
    );
}

#[tokio::test]
async fn a_thrashing_domain_lands_one_request_per_batch() {
    let fx = Fixture::new().await;
    let lander = fx.lander(
        Arc::new(NoopHooks),
        Arc::new(rill_lander::AlwaysGreen),
        fast_policy(),
    );
    let mut events = lander.subscribe();
    let body = lines(10);
    let base = base(
        &fx,
        &[
            ("hot/f", &body),
            (
                ".rill/landing.toml",
                "[throttle]\nconflict_rate = 0.5\nmin_attempts = 2\n",
            ),
        ],
    )
    .await;
    let (_, a) = fx
        .commit(
            Some(base),
            &[("hot/f", &body.replace("line 1\n", "a\n"))],
            "a",
        )
        .await;
    let (_, b) = fx
        .commit(
            Some(base),
            &[("hot/f", &body.replace("line 1\n", "b\n"))],
            "b",
        )
        .await;
    for cv in [&a, &b] {
        lander
            .enqueue(request(cv, RiskClass::Trivial))
            .await
            .unwrap();
    }
    lander.run_once().await.unwrap();

    let (_, c) = fx.commit(Some(base), &[("hot/c", "c")], "c").await;
    let (_, d) = fx.commit(Some(base), &[("hot/d", "d")], "d").await;
    let tickets = [
        lander
            .enqueue(request(&c, RiskClass::Trivial))
            .await
            .unwrap(),
        lander
            .enqueue(request(&d, RiskClass::Trivial))
            .await
            .unwrap(),
    ];
    lander.run_once().await.unwrap();
    for t in tickets {
        assert!(matches!(
            status(&lander, t).await,
            LandStatus::Landed { .. }
        ));
    }
    let mut throttled = false;
    while let Ok(event) = events.try_recv() {
        throttled |= matches!(event, LandEvent::Throttled { ref domain, .. } if domain == "hot/");
    }
    assert!(throttled, "the sponsor hears why");
}

#[tokio::test]
async fn sponsors_and_the_queue_are_bounded_and_the_queue_reports_positions() {
    let fx = Fixture::new().await;
    let policy = LandPolicy {
        max_sponsor_in_flight: 2,
        max_queued: 4,
        ..fast_policy()
    };
    let lander = fx.lander(
        Arc::new(NoopHooks),
        Arc::new(rill_lander::AlwaysGreen),
        policy,
    );
    let base = base(&fx, &[("base", "0")]).await;
    let mut tickets = Vec::new();
    for i in 0..3 {
        let (_, cv) = fx.commit(Some(base), &[(&format!("s{i}"), "s")], "s").await;
        let req = rill_lander::LandRequest::new(
            cv.change_id,
            cv.version_hash(),
            Principal::agent(format!("bot{i}")),
            RiskClass::Trivial,
        )
        .sponsored_by(Some("user:swarm".into()));
        let queued = lander.enqueue(req).await;
        match i {
            2 => assert!(
                matches!(queued, Err(rill_lander::Error::TooManyRequests { .. })),
                "{queued:?}"
            ),
            _ => tickets.push(queued.unwrap()),
        }
    }
    for i in 0..3 {
        let (_, cv) = fx.commit(Some(base), &[(&format!("h{i}"), "h")], "h").await;
        let queued = lander.enqueue(request(&cv, RiskClass::Trivial)).await;
        match i {
            2 => assert!(
                matches!(queued, Err(rill_lander::Error::QueueFull { .. })),
                "{queued:?}"
            ),
            _ => tickets.push(queued.unwrap()),
        }
    }
    let mut places = Vec::new();
    for t in &tickets {
        places.push(lander.position(*t).await.unwrap().unwrap().0);
    }
    assert!(
        places[2] < 2 && places[3] < 2,
        "humans before agents: {places:?}"
    );
    assert!(places[0] >= 2 && places[1] >= 2, "{places:?}");
    lander.run_once().await.unwrap();
    assert!(lander.position(tickets[1]).await.unwrap().is_none());
    assert!(lander.timings().interval < Duration::from_secs(5));
}

/// Files under a top-level directory make up target `//<dir>`.
struct DirTargets(rill_core::TreeOps);

#[async_trait]
impl rill_lander::TargetGraph for DirTargets {
    async fn touched(
        &self,
        root: rill_types::TreeHash,
        base: Option<rill_types::TreeHash>,
    ) -> rill_lander::Result<Option<std::collections::BTreeSet<String>>> {
        let changed = rill_core::diff_trees(&self.0, base, Some(root))
            .await
            .unwrap();
        Ok(Some(
            changed
                .iter()
                .filter_map(|e| e.path.as_str().split_once('/'))
                .map(|(dir, _)| format!("//{dir}"))
                .collect(),
        ))
    }
}

/// Records which files each verified tree holds.
struct SeenFiles(rill_core::TreeOps, tokio::sync::Mutex<Vec<Vec<String>>>);

#[async_trait]
impl Verifier for SeenFiles {
    async fn verify(&self, request: VerifyRequest) -> rill_lander::Result<VerificationOutcome> {
        let app = rill_types::RepoPathBuf::new("app").unwrap();
        let names = self.0.list_dir(request.root, &app).await.unwrap();
        self.1
            .lock()
            .await
            .push(names.into_iter().map(|(n, _)| n).collect());
        Ok(VerificationOutcome::green(""))
    }
}

#[tokio::test]
async fn requests_touching_one_build_target_are_verified_together() {
    let fx = Fixture::new().await;
    let seen = Arc::new(SeenFiles(fx.trees.clone(), Default::default()));
    let lander = Arc::new(
        fx.bare_lander(Arc::new(NoopHooks), seen.clone(), fast_policy())
            .with_targets(Arc::new(DirTargets(fx.trees.clone()))),
    );
    let base = base(&fx, &[("app/main.rs", "0"), ("docs/a.md", "0")]).await;
    for (name, file) in [("lib", "app/lib.rs"), ("util", "app/util.rs")] {
        let (_, cv) = fx.commit(Some(base), &[(file, name)], name).await;
        lander
            .enqueue(request(&cv, RiskClass::Normal))
            .await
            .unwrap();
    }
    lander.run_once().await.unwrap();
    assert_eq!(fx.head().await.0, TrunkSeq(3));
    let seen = seen.1.lock().await;
    assert!(
        seen.iter()
            .any(|files| files.contains(&"lib.rs".to_string())
                && files.contains(&"util.rs".to_string())),
        "the second request was verified on top of the first: {seen:?}"
    );
}
