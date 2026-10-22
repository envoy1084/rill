//! The lander's dependencies on other services, as traits.
//!
//! The lander never depends on the review crate or the execution runner
//! directly: the server wires concrete implementations in, and tests use
//! the no-op and always-green ones here.

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use rill_store::repo::ReviewSnapshot;
use rill_types::{
    ChangeId, ChangeVersionHash, LabelVote, Principal, RepoPathBuf, RiskClass, TreeHash, TrunkSeq,
};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// What review says about landing a change now.
#[derive(Debug, Clone, Default)]
pub struct Approval {
    /// The review as read; the landing commits only if it is unchanged.
    /// `None` for a change without review (reverts, direct lands).
    pub review: Option<ReviewSnapshot>,
    /// Why the change may not land; `None` when it may.
    pub refusal: Option<String>,
    /// BLAKE3 of the submit policy that judged it.
    pub policy: Option<[u8; 32]>,
    /// The votes that policy saw.
    pub votes: Vec<LabelVote>,
}

/// Review-state transitions the lander drives.
#[async_trait]
pub trait ReviewHooks: Send + Sync {
    /// The change is being processed by the lander.
    async fn mark_landing(&self, change: ChangeId) -> Result<()>;

    /// `version` of the change landed at `seq`; the land transaction has
    /// already recorded it, so this only notifies.
    async fn mark_landed(
        &self,
        change: ChangeId,
        seq: TrunkSeq,
        version: ChangeVersionHash,
    ) -> Result<()>;

    /// Landing failed; the owner must act (resolve a conflict, fix a red
    /// verification) and resubmit.
    async fn return_to_owner(&self, change: ChangeId, reason: String) -> Result<()>;

    /// Changes stacked directly on `change`.
    async fn dependents_of(&self, change: ChangeId) -> Result<Vec<ChangeId>>;

    /// `change` landed as `landed`: rebase the changes stacked on it onto
    /// it, bottom up, as new versions of those changes. The default is for
    /// changes without review, which have no stacks.
    async fn restack(
        &self,
        _change: ChangeId,
        _landed: ChangeVersionHash,
    ) -> Result<Vec<Restacked>> {
        Ok(Vec::new())
    }

    /// An incident about `change`'s landing was opened, or its automatic
    /// revert could not land: tell its owner (and sponsor).
    async fn notify_incident(
        &self,
        _change: ChangeId,
        _incident: &rill_types::Incident,
    ) -> Result<()> {
        Ok(())
    }

    /// Whether `requester` may land `version` of `change` now, judged on
    /// the review as read; the lander lands only if that review is still
    /// what it was when the landing commits. The default is for changes
    /// without review.
    async fn approve(
        &self,
        _change: ChangeId,
        _version: ChangeVersionHash,
        _requester: &Principal,
    ) -> Result<Approval> {
        Ok(Approval::default())
    }
}

/// A stacked change rebased onto the landed change below it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restacked {
    pub change: ChangeId,
    /// Its new version.
    pub version: ChangeVersionHash,
    /// Paths the rebase left in conflict, kept in the version as data.
    pub conflicts: Vec<RepoPathBuf>,
}

/// Hooks that do nothing; for tests and repositories without review.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopHooks;

#[async_trait]
impl ReviewHooks for NoopHooks {
    async fn mark_landing(&self, _change: ChangeId) -> Result<()> {
        Ok(())
    }

    async fn mark_landed(
        &self,
        _change: ChangeId,
        _seq: TrunkSeq,
        _version: ChangeVersionHash,
    ) -> Result<()> {
        Ok(())
    }

    async fn return_to_owner(&self, _change: ChangeId, _reason: String) -> Result<()> {
        Ok(())
    }

    async fn dependents_of(&self, _change: ChangeId) -> Result<Vec<ChangeId>> {
        Ok(Vec::new())
    }
}

/// Something that failed in a verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// `step:<name>[#shard]` or `test:<id>`.
    pub target: String,
    /// The step it belongs to (for a test case, the step that ran it).
    pub step: String,
    /// Its flake score in per mille (0 to 1000); 0 without a history.
    pub flake_permille: u32,
}

/// Result of verifying one tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationOutcome {
    /// True when verification passed.
    pub green: bool,
    /// Human-readable detail: failing step, log excerpt, cache status.
    pub details: String,
    /// What failed, when red.
    pub failures: Vec<Failure>,
    /// It failed at first and passed when its flaky failures were run
    /// again: a flake, not a break.
    pub flaky: bool,
}

impl VerificationOutcome {
    /// A passing outcome with `details`.
    pub fn green(details: impl Into<String>) -> Self {
        VerificationOutcome {
            green: true,
            details: details.into(),
            failures: Vec::new(),
            flaky: false,
        }
    }

    /// A failing outcome with `details`.
    pub fn red(details: impl Into<String>) -> Self {
        VerificationOutcome {
            green: false,
            details: details.into(),
            failures: Vec::new(),
            flaky: false,
        }
    }

    /// The same outcome, naming what failed.
    pub fn with_failures(mut self, failures: Vec<Failure>) -> Self {
        self.failures = failures;
        self
    }

    /// The step names its failures belong to.
    pub fn failing_steps(&self) -> std::collections::BTreeSet<String> {
        self.failures.iter().map(|f| f.step.clone()).collect()
    }
}

/// Why a tree is being verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyStage {
    /// Before landing, gating the change.
    PreLand,
    /// After landing, following trunk.
    PostLand,
}

/// A tree to verify and what it changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyRequest {
    /// The root tree of `version`.
    pub root: TreeHash,
    pub version: ChangeVersionHash,
    /// The tree `root` was made against, so checks whose inputs did not
    /// change can be skipped; `None` checks everything.
    pub base: Option<TreeHash>,
    /// The change's risk class, when gating one.
    pub risk: Option<RiskClass>,
    pub stage: VerifyStage,
}

/// Runs verification for a tree. The server adapts `rill-exec` to this.
#[async_trait]
pub trait Verifier: Send + Sync {
    async fn verify(&self, request: VerifyRequest) -> Result<VerificationOutcome>;

    /// Whether going from `base` to `root` could change the outcome of
    /// `steps`: it touched their inputs. `true` when that cannot be told,
    /// which is the default.
    async fn could_affect(
        &self,
        _root: TreeHash,
        _base: Option<TreeHash>,
        _steps: &std::collections::BTreeSet<String>,
    ) -> Result<bool> {
        Ok(true)
    }
}

/// A verifier that passes everything.
#[derive(Debug, Default, Clone, Copy)]
pub struct AlwaysGreen;

#[async_trait]
impl Verifier for AlwaysGreen {
    async fn verify(&self, _request: VerifyRequest) -> Result<VerificationOutcome> {
        Ok(VerificationOutcome::green("no verification configured"))
    }
}

/// A verifier built from a closure over the request, for tests and
/// simple policies.
pub struct FnVerifier<F>(pub F);

#[async_trait]
impl<F, Fut> Verifier for FnVerifier<F>
where
    F: Fn(VerifyRequest) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<VerificationOutcome>> + Send,
{
    async fn verify(&self, request: VerifyRequest) -> Result<VerificationOutcome> {
        (self.0)(request).await
    }
}

/// A `regenerate` merge driver's command to run on a merged tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegenerateRequest {
    /// The driver's name, for labels and logs.
    pub driver: String,
    /// The merged tree, holding the head's version of every output.
    pub root: TreeHash,
    pub version: ChangeVersionHash,
    pub command: Vec<String>,
    /// Globs of the files the command reads; empty is the whole tree.
    pub inputs: Vec<String>,
    /// The files it rebuilds, read back after it runs.
    pub outputs: Vec<RepoPathBuf>,
    pub env: BTreeMap<String, String>,
    pub platform: BTreeMap<String, String>,
    pub timeout: Duration,
}

/// What a regenerate command produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Regenerated {
    /// Every output, by path.
    Files(BTreeMap<RepoPathBuf, Bytes>),
    /// It failed, or left an output out.
    Failed(String),
}

/// Runs `regenerate` merge drivers. The server adapts `rill-exec` to this.
#[async_trait]
pub trait Regenerator: Send + Sync {
    async fn regenerate(&self, request: RegenerateRequest) -> Result<Regenerated>;
}

/// A regenerator for servers without remote execution: every
/// regeneration fails, so those conflicts go to the owner.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoRegenerator;

#[async_trait]
impl Regenerator for NoRegenerator {
    async fn regenerate(&self, _request: RegenerateRequest) -> Result<Regenerated> {
        Ok(Regenerated::Failed(
            "this server runs no actions (verification is off)".into(),
        ))
    }
}

/// The ladder's last rung before the owner: hand a conflict no driver
/// resolved to a resolver agent. The agent's resolution is a new version
/// of the change that comes back through the queue.
#[async_trait]
pub trait ConflictResolver: Send + Sync {
    /// Offer `version` of `change`, conflicted at `paths` when rebased
    /// onto `onto`. Returns who took it, for the note to the owner, or
    /// `None` when no agent did.
    async fn dispatch(
        &self,
        change: ChangeId,
        version: ChangeVersionHash,
        onto: ChangeVersionHash,
        paths: &[RepoPathBuf],
    ) -> Result<Option<String>>;
}

/// The build targets a change touches, for conflict domains finer than
/// paths. The server adapts the verify plan's `[graph]` query to this.
#[async_trait]
pub trait TargetGraph: Send + Sync {
    /// Targets whose hash differs between `base` and `root`; `None` when
    /// the tree declares no build graph or it cannot be queried.
    async fn touched(
        &self,
        root: TreeHash,
        base: Option<TreeHash>,
    ) -> Result<Option<std::collections::BTreeSet<String>>>;
}

/// Hands a trunk break to a fix agent. Dispatch to real agents arrives
/// with the reviewer fleet; until then the server has none.
#[async_trait]
pub trait FixDispatcher: Send + Sync {
    /// Offer `incident`; returns who took it, or `None`.
    async fn dispatch(&self, incident: &rill_types::Incident) -> Result<Option<String>>;
}
