//! Post-land verification: k-ary bisection over a long red range.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use common::{fast_policy, Fixture};
use rill_lander::{NoopHooks, PostLandVerifier, VerificationOutcome, Verifier, VerifyRequest};
use rill_types::{RepoPathBuf, TrunkSeq};

/// Red once a `bad` file exists; records how many verifications overlap.
struct Watching {
    trees: rill_core::TreeOps,
    calls: AtomicUsize,
    running: AtomicUsize,
    peak: AtomicUsize,
}

#[async_trait]
impl Verifier for Watching {
    async fn verify(&self, request: VerifyRequest) -> rill_lander::Result<VerificationOutcome> {
        let root = request.root;
        self.calls.fetch_add(1, Ordering::SeqCst);
        let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        self.running.fetch_sub(1, Ordering::SeqCst);
        let bad = self
            .trees
            .lookup(root, &RepoPathBuf::new("bad").unwrap())
            .await
            .unwrap()
            .is_some();
        Ok(if bad {
            VerificationOutcome::red("bad file")
        } else {
            VerificationOutcome::green("ok")
        })
    }
}

#[tokio::test]
async fn bisection_checks_many_positions_per_round() {
    let fx = Fixture::new().await;
    let mut tip = None;
    for i in 1..=64u32 {
        let name = format!("f{i}");
        let mut files = vec![(name.as_str(), "x")];
        if i == 37 {
            files.push(("bad", "!"));
        }
        let (hash, _) = fx.commit(tip, &files, &name).await;
        fx.repo.land_on_trunk(tip, hash).await.unwrap();
        tip = Some(hash);
    }
    let verifier = Arc::new(Watching {
        trees: fx.trees.clone(),
        calls: AtomicUsize::new(0),
        running: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
    });
    let lander = fx.lander(Arc::new(NoopHooks), verifier.clone(), fast_policy());
    let worker = PostLandVerifier::new(lander, 64);

    let sweep = worker.run_once().await.unwrap().unwrap();
    assert_eq!(sweep.culprit, Some(TrunkSeq(37)));
    assert!(
        verifier.calls.load(Ordering::SeqCst) <= 15,
        "the tip and at most two rounds of seven, not {}",
        verifier.calls.load(Ordering::SeqCst)
    );
    assert!(
        verifier.peak.load(Ordering::SeqCst) > 1,
        "a round verifies in parallel"
    );
}
