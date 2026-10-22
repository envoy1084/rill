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
    pub incidents: Mutex<Vec<(ChangeId, rill_types::Incident)>>,
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

    async fn notify_incident(
        &self,
        change: ChangeId,
        incident: &rill_types::Incident,
    ) -> rill_lander::Result<()> {
        self.incidents.lock().await.push((change, incident.clone()));
        Ok(())
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

/// A regenerator that writes, into each output, the names under `deps/`
/// in the merged tree, one per line; or fails with `fail`.
pub struct ListingRegenerator {
    pub trees: rill_core::TreeOps,
    pub fail: Option<String>,
    pub seen: Mutex<Vec<rill_lander::RegenerateRequest>>,
}

#[async_trait]
impl rill_lander::Regenerator for ListingRegenerator {
    async fn regenerate(
        &self,
        request: rill_lander::RegenerateRequest,
    ) -> rill_lander::Result<rill_lander::Regenerated> {
        self.seen.lock().await.push(request.clone());
        if let Some(why) = &self.fail {
            return Ok(rill_lander::Regenerated::Failed(why.clone()));
        }
        let deps = self
            .trees
            .list_dir(request.root, &RepoPathBuf::new("deps").unwrap())
            .await
            .unwrap();
        let listing: String = deps.iter().map(|(name, _)| format!("{name}\n")).collect();
        Ok(rill_lander::Regenerated::Files(
            request
                .outputs
                .iter()
                .map(|p| (p.clone(), bytes::Bytes::from(listing.clone())))
                .collect(),
        ))
    }
}

/// A resolver that takes every conflict it is offered.
#[derive(Default)]
pub struct TakingResolver {
    pub offered: Mutex<Vec<Vec<RepoPathBuf>>>,
}

#[async_trait]
impl rill_lander::ConflictResolver for TakingResolver {
    async fn dispatch(
        &self,
        _change: ChangeId,
        _version: ChangeVersionHash,
        _onto: ChangeVersionHash,
        paths: &[RepoPathBuf],
    ) -> rill_lander::Result<Option<String>> {
        self.offered.lock().await.push(paths.to_vec());
        Ok(Some("agent:fixer".into()))
    }
}

/// Hooks for one declared stack: reports its dependents and records
/// restacks.
pub struct StackHooks {
    pub stack: Vec<ChangeId>,
    pub restacked: Mutex<Vec<(ChangeId, ChangeVersionHash)>>,
    pub returned: Mutex<Vec<(ChangeId, String)>>,
}

impl StackHooks {
    pub fn new(stack: Vec<ChangeId>) -> Self {
        StackHooks {
            stack,
            restacked: Mutex::new(Vec::new()),
            returned: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ReviewHooks for StackHooks {
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

    async fn return_to_owner(&self, change: ChangeId, reason: String) -> rill_lander::Result<()> {
        self.returned.lock().await.push((change, reason));
        Ok(())
    }

    async fn dependents_of(&self, change: ChangeId) -> rill_lander::Result<Vec<ChangeId>> {
        Ok(self
            .stack
            .iter()
            .position(|c| *c == change)
            .map(|at| self.stack[at + 1..].to_vec())
            .unwrap_or_default())
    }

    async fn restack(
        &self,
        change: ChangeId,
        landed: ChangeVersionHash,
    ) -> rill_lander::Result<Vec<rill_lander::Restacked>> {
        self.restacked.lock().await.push((change, landed));
        Ok(Vec::new())
    }
}

/// Like [`bad_file_verifier`], but selecting like a verify plan: with a
/// base, the check on `bad` runs only when `bad` changed since it.
pub fn selecting_verifier(fx: &Fixture) -> Arc<dyn Verifier> {
    let trees = fx.trees.clone();
    Arc::new(FnVerifier(move |request: VerifyRequest| {
        let trees = trees.clone();
        async move {
            let path = RepoPathBuf::new("bad").unwrap();
            let now = trees.lookup(request.root, &path).await.unwrap();
            let touched = match request.base {
                Some(base) => trees.lookup(base, &path).await.unwrap() != now,
                None => true,
            };
            Ok(if now.is_some() && touched {
                VerificationOutcome::red("bad file present")
            } else {
                VerificationOutcome::green("ok")
            })
        }
    }))
}

/// A plan of named steps: step `x` fails while a file `bad_x` exists. With
/// a base, a step runs only when its `bad_x` changed since it, and a
/// change affects step `x` when it touches `bad_x`, as with real inputs.
pub struct StepsVerifier {
    pub trees: rill_core::TreeOps,
    pub steps: Vec<&'static str>,
    pub calls: AtomicUsize,
}

impl StepsVerifier {
    async fn file(
        &self,
        root: Option<rill_types::TreeHash>,
        step: &str,
    ) -> Option<rill_types::EntryKind> {
        let root = root?;
        self.trees
            .lookup(root, &RepoPathBuf::new(format!("bad_{step}")).unwrap())
            .await
            .unwrap()
    }
}

#[async_trait]
impl Verifier for StepsVerifier {
    async fn verify(&self, request: VerifyRequest) -> rill_lander::Result<VerificationOutcome> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut failures = Vec::new();
        for step in &self.steps {
            let now = self.file(Some(request.root), step).await;
            let runs = match request.base {
                Some(base) => self.file(Some(base), step).await != now,
                None => true,
            };
            if runs && now.is_some() {
                failures.push(rill_lander::Failure {
                    target: format!("step:{step}"),
                    step: step.to_string(),
                    flake_permille: 0,
                });
            }
        }
        Ok(match failures.is_empty() {
            true => VerificationOutcome::green("ok"),
            false => VerificationOutcome::red("steps failed").with_failures(failures),
        })
    }

    async fn could_affect(
        &self,
        root: rill_types::TreeHash,
        base: Option<rill_types::TreeHash>,
        steps: &std::collections::BTreeSet<String>,
    ) -> rill_lander::Result<bool> {
        for step in steps {
            if self.file(Some(root), step).await != self.file(base, step).await {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
