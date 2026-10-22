//! The lander's dependencies on other services, as traits.
//!
//! The lander never depends on the review crate or the execution runner
//! directly: the server wires concrete implementations in, and tests use
//! the no-op and always-green ones here.

use async_trait::async_trait;
use rill_store::repo::ReviewSnapshot;
use rill_types::{ChangeId, ChangeVersionHash, LabelVote, Principal, TreeHash, TrunkSeq};
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

/// Result of verifying one tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationOutcome {
    /// True when verification passed.
    pub green: bool,
    /// Human-readable detail: failing step, log excerpt, cache status.
    pub details: String,
}

impl VerificationOutcome {
    /// A passing outcome with `details`.
    pub fn green(details: impl Into<String>) -> Self {
        VerificationOutcome {
            green: true,
            details: details.into(),
        }
    }

    /// A failing outcome with `details`.
    pub fn red(details: impl Into<String>) -> Self {
        VerificationOutcome {
            green: false,
            details: details.into(),
        }
    }
}

/// Runs verification for a tree. The server adapts `rill-exec` to this;
/// the version hash is passed so implementations can attribute results.
#[async_trait]
pub trait Verifier: Send + Sync {
    /// Verify `root`, the root tree of `version`.
    async fn verify(
        &self,
        root: TreeHash,
        version: ChangeVersionHash,
    ) -> Result<VerificationOutcome>;
}

/// A verifier that passes everything.
#[derive(Debug, Default, Clone, Copy)]
pub struct AlwaysGreen;

#[async_trait]
impl Verifier for AlwaysGreen {
    async fn verify(
        &self,
        _root: TreeHash,
        _version: ChangeVersionHash,
    ) -> Result<VerificationOutcome> {
        Ok(VerificationOutcome::green("no verification configured"))
    }
}

/// A verifier built from a closure over the root tree, for tests and
/// simple policies.
pub struct FnVerifier<F>(pub F);

#[async_trait]
impl<F, Fut> Verifier for FnVerifier<F>
where
    F: Fn(TreeHash, ChangeVersionHash) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<VerificationOutcome>> + Send,
{
    async fn verify(
        &self,
        root: TreeHash,
        version: ChangeVersionHash,
    ) -> Result<VerificationOutcome> {
        (self.0)(root, version).await
    }
}
