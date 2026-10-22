//! Review hooks and verifiers that record or refuse.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use rill_lander::{FnVerifier, ReviewHooks, VerificationOutcome, Verifier, VerifyRequest};
use rill_types::{ChangeId, ChangeVersionHash, RepoPathBuf, TrunkSeq};
use tokio::sync::Mutex;

use crate::common::Fixture;

#[derive(Default)]
pub struct RecordingHooks {
    pub landed: Mutex<Vec<(ChangeId, TrunkSeq)>>,
    pub returned: Mutex<Vec<(ChangeId, String)>>,
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

pub struct CountingVerifier {
    pub calls: AtomicUsize,
    pub green: bool,
}

#[async_trait]
impl Verifier for CountingVerifier {
    async fn verify(&self, _request: VerifyRequest) -> rill_lander::Result<VerificationOutcome> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(if self.green {
            VerificationOutcome::green("ok")
        } else {
            VerificationOutcome::red("tests failed")
        })
    }
}

/// Hooks whose review refuses to move any change to `Landing`.
pub struct RefusingHooks;

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

/// Verifier that is red whenever `bad` exists at the root.
pub fn bad_file_verifier(fx: &Fixture) -> Arc<dyn Verifier> {
    let trees = fx.trees.clone();
    Arc::new(FnVerifier(move |request: VerifyRequest| {
        let root = request.root;
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
