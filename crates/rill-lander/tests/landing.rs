//! End-to-end behaviour of the landing pipeline over memory stores.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rill_core::{read_content, ChangeBuilder, CommitGraph, DeriveManager, RootTree, TreeOps};
use rill_lander::land::followups;
use rill_lander::{
    revert, AlwaysGreen, FnVerifier, LandEvent, LandPolicy, LandRequest, LandStatus, Lander,
    NoopHooks, PostLandVerifier, ReviewHooks, VerificationOutcome, VerificationStatus, Verifier,
};
use rill_store::{RepoStore, Stores};
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, EntryKind, Principal, RepoPathBuf, RiskClass,
    TreeHash, TrunkSeq,
};
use tokio::sync::Mutex;

struct Fixture {
    repo: RepoStore,
    derive: Arc<DeriveManager>,
    trees: TreeOps,
}

impl Fixture {
    async fn new() -> Self {
        let stores = Stores::memory();
        let config = stores.create_repo("r", "").await.unwrap();
        let repo = stores.repo(config.id);
        let trees = TreeOps::new(repo.objects().clone(), 64);
        let derive = DeriveManager::new(repo.clone(), trees.clone(), 64);
        Fixture {
            repo,
            derive,
            trees,
        }
    }

    fn lander(
        &self,
        hooks: Arc<dyn ReviewHooks>,
        verifier: Arc<dyn Verifier>,
        policy: LandPolicy,
    ) -> Arc<Lander> {
        let graph = Arc::new(CommitGraph::new(self.repo.clone(), self.derive.clone()));
        Lander::new(
            self.repo.clone(),
            self.derive.clone(),
            graph,
            hooks,
            verifier,
            policy,
        )
    }

    async fn commit(
        &self,
        parent: Option<ChangeVersionHash>,
        files: &[(&str, &str)],
        message: &str,
    ) -> (ChangeVersionHash, ChangeVersion) {
        let mut builder = ChangeBuilder::new(Principal::human("alice")).message(message);
        if let Some(parent) = parent {
            builder = builder.parent(parent);
        }
        for (path, content) in files {
            builder = builder.file(path, content.as_bytes().to_vec()).unwrap();
        }
        builder.commit(&self.repo).await.unwrap()
    }

    async fn delete(
        &self,
        parent: ChangeVersionHash,
        path: &str,
        message: &str,
    ) -> (ChangeVersionHash, ChangeVersion) {
        ChangeBuilder::new(Principal::human("alice"))
            .message(message)
            .parent(parent)
            .delete(path)
            .unwrap()
            .commit(&self.repo)
            .await
            .unwrap()
    }

    async fn root(&self, version: &ChangeVersionHash) -> TreeHash {
        self.derive.get::<RootTree>(version).await.unwrap()
    }

    async fn head(&self) -> (TrunkSeq, ChangeVersionHash) {
        self.repo.trunk_head().await.unwrap().unwrap()
    }

    async fn file(&self, root: TreeHash, path: &str) -> Option<String> {
        match self
            .trees
            .lookup(root, &RepoPathBuf::new(path).unwrap())
            .await
            .unwrap()
        {
            Some(EntryKind::File { content, .. }) => {
                let bytes = read_content(self.trees.objects().as_ref(), &content)
                    .await
                    .unwrap();
                Some(String::from_utf8(bytes.to_vec()).unwrap())
            }
            _ => None,
        }
    }
}

fn request(cv: &ChangeVersion, risk: RiskClass) -> LandRequest {
    LandRequest::new(
        cv.change_id,
        cv.version_hash(),
        Principal::human("alice"),
        risk,
    )
}

fn fast_policy() -> LandPolicy {
    LandPolicy {
        batch_window: Duration::from_millis(10),
        ..LandPolicy::default()
    }
}

#[derive(Default)]
struct RecordingHooks {
    landed: Mutex<Vec<(ChangeId, TrunkSeq)>>,
    returned: Mutex<Vec<(ChangeId, String)>>,
}

#[async_trait]
impl ReviewHooks for RecordingHooks {
    async fn mark_landing(&self, _change: ChangeId) -> rill_lander::Result<()> {
        Ok(())
    }

    async fn mark_landed(
        &self,
        change: ChangeId,
        seq: TrunkSeq,
        _version: ChangeVersionHash,
    ) -> rill_lander::Result<()> {
        self.landed.lock().await.push((change, seq));
        Ok(())
    }

    async fn return_to_owner(&self, change: ChangeId, reason: String) -> rill_lander::Result<()> {
        self.returned.lock().await.push((change, reason));
        Ok(())
    }

    async fn dependents_of(&self, _change: ChangeId) -> rill_lander::Result<Vec<ChangeId>> {
        Ok(Vec::new())
    }
}

struct CountingVerifier {
    calls: AtomicUsize,
    green: bool,
}

#[async_trait]
impl Verifier for CountingVerifier {
    async fn verify(
        &self,
        _root: TreeHash,
        _version: ChangeVersionHash,
    ) -> rill_lander::Result<VerificationOutcome> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(if self.green {
            VerificationOutcome::green("ok")
        } else {
            VerificationOutcome::red("tests failed")
        })
    }
}

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

/// Hooks whose review refuses to move any change to `Landing`.
struct RefusingHooks;

#[async_trait]
impl ReviewHooks for RefusingHooks {
    async fn mark_landing(&self, change: ChangeId) -> rill_lander::Result<()> {
        Err(rill_lander::Error::Hook(format!("{change} is abandoned")))
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

    // Each landing queued its follow-up work in the same transaction.
    let derive = rill_jobs::Queue::new(fx.repo.clone(), followups::DERIVE_QUEUE);
    let index = rill_jobs::Queue::new(fx.repo.clone(), followups::INDEX_QUEUE);
    assert_eq!(derive.stats().await.unwrap().enqueued, 2);
    assert_eq!(index.stats().await.unwrap().enqueued, 2);
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

/// Verifier that is red whenever `bad` exists at the root.
fn bad_file_verifier(fx: &Fixture) -> Arc<dyn Verifier> {
    let trees = fx.trees.clone();
    Arc::new(FnVerifier(move |root: TreeHash, _| {
        let trees = trees.clone();
        async move {
            let bad = trees
                .lookup(root, &RepoPathBuf::new("bad").unwrap())
                .await
                .map_err(rill_lander::Error::from)?;
            Ok(if bad.is_some() {
                VerificationOutcome::red("bad file present")
            } else {
                VerificationOutcome::green("ok")
            })
        }
    }))
}

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

    let sweep = worker.run_once().await.unwrap().unwrap();
    assert_eq!((sweep.from, sweep.to), (TrunkSeq(0), TrunkSeq(5)));
    assert!(!sweep.green);
    assert_eq!(sweep.culprit, Some(TrunkSeq(3)));
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
            (1, VerificationStatus::Green, None, true),
            (2, VerificationStatus::Green, None, false),
            (3, VerificationStatus::Red, Some(3), false),
            (4, VerificationStatus::Red, Some(3), true),
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
    let mut events = lander.subscribe();
    let worker = PostLandVerifier::new(lander.clone(), 8);
    let sweep = worker.run_once().await.unwrap().unwrap();
    assert_eq!(sweep.culprit, Some(TrunkSeq(2)));

    let reverted = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let LandEvent::Reverted { culprit, revert } = events.recv().await.unwrap() {
                return (culprit, revert);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(reverted, (TrunkSeq(2), TrunkSeq(4)));

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
async fn run_forever_processes_until_shutdown() {
    let fx = Fixture::new().await;
    let lander = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    let shutdown = tokio_util::sync::CancellationToken::new();
    let task = tokio::spawn(lander.clone().run_forever(shutdown.clone()));

    let (_, cv) = fx.commit(None, &[("a", "a")], "a").await;
    let ticket = lander
        .enqueue(request(&cv, RiskClass::Trivial))
        .await
        .unwrap();
    let status = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status = lander.queue().status(ticket).await.unwrap().unwrap();
            if status.is_terminal() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(status, LandStatus::Landed { seq: TrunkSeq(1) });
    shutdown.cancel();
    task.await.unwrap();
}

#[tokio::test]
async fn a_standby_lander_waits_for_the_lease() {
    let fx = Fixture::new().await;
    let active = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    let standby = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    let (base, _) = fx.commit(None, &[("base", "0")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();
    assert!(active.lease().await.unwrap().is_some());

    let (_, a) = fx.commit(Some(base), &[("a.txt", "a")], "a").await;
    let ticket = standby
        .enqueue(request(&a, RiskClass::Trivial))
        .await
        .unwrap();
    assert_eq!(
        standby.run_once().await.unwrap(),
        0,
        "standby leaves it queued"
    );
    assert_eq!(fx.head().await.0, TrunkSeq(1));

    active.release().await;
    assert_eq!(standby.run_once().await.unwrap(), 1);
    assert!(matches!(
        standby.queue().status(ticket).await.unwrap().unwrap(),
        LandStatus::Landed { seq: TrunkSeq(2) }
    ));
}

#[tokio::test]
async fn a_request_outlives_the_lander_that_accepted_it() {
    let fx = Fixture::new().await;
    let (base, _) = fx.commit(None, &[("base", "0")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();
    let (_, a) = fx.commit(Some(base), &[("a.txt", "a")], "a").await;

    let crashed = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    assert!(crashed.lease().await.unwrap().is_some());
    let ticket = crashed
        .enqueue(request(&a, RiskClass::Trivial))
        .await
        .unwrap();
    assert_eq!(
        crashed
            .enqueue(request(&a, RiskClass::Trivial))
            .await
            .unwrap(),
        ticket,
        "asking again returns the same request"
    );
    // The process goes away before serving the request; its successor
    // takes the lease and finds the request in the store.
    crashed.release().await;
    drop(crashed);

    let successor = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    assert_eq!(successor.run_once().await.unwrap(), 1);
    assert_eq!(
        successor.queue().status(ticket).await.unwrap(),
        Some(LandStatus::Landed { seq: TrunkSeq(2) })
    );
    assert!(successor.queue().is_empty().await.unwrap());
}
