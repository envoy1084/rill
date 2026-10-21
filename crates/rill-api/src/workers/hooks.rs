//! The lander's view of the review state machine.

use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use rill_lander::ReviewHooks;
use rill_review::error::Error as ReviewError;
use rill_review::service::ReviewService;
use rill_types::{ChangeId, ChangeVersionHash, ThreadAnchor, TrunkSeq};

/// Adapts [`ReviewService`] to [`ReviewHooks`]. Holds the swappable
/// service handle so a policy reload is seen by in-flight landings.
pub struct ReviewHooksAdapter {
    review: Arc<ArcSwap<ReviewService>>,
}

impl ReviewHooksAdapter {
    /// Hooks over the given review service handle.
    pub fn new(review: Arc<ArcSwap<ReviewService>>) -> Self {
        ReviewHooksAdapter { review }
    }

    fn service(&self) -> Arc<ReviewService> {
        self.review.load_full()
    }
}

fn hook_err(e: ReviewError) -> rill_lander::Error {
    rill_lander::Error::Hook(e.to_string())
}

#[async_trait]
impl ReviewHooks for ReviewHooksAdapter {
    async fn mark_landing(&self, change: ChangeId) -> rill_lander::Result<()> {
        match self.service().mark_landing(&change).await {
            Ok(_) => Ok(()),
            // A change landed without going through review (a revert, or a
            // direct land in dev mode) has no review state to move.
            Err(ReviewError::ReviewNotFound(_)) => Ok(()),
            Err(e) => Err(hook_err(e)),
        }
    }

    async fn mark_landed(
        &self,
        change: ChangeId,
        seq: TrunkSeq,
        _version: ChangeVersionHash,
    ) -> rill_lander::Result<()> {
        self.service().announce_landed(&change, seq);
        Ok(())
    }

    async fn return_to_owner(&self, change: ChangeId, reason: String) -> rill_lander::Result<()> {
        let service = self.service();
        match service.landing_failed(&change).await {
            Ok(_) => {}
            Err(ReviewError::ReviewNotFound(_)) => return Ok(()),
            // Not in `Landing` (for example the land failed before the
            // review moved): just re-evaluate so the status is honest.
            Err(ReviewError::InvalidStatus { .. }) => {
                service.refresh(&change).await.map_err(hook_err)?;
            }
            Err(e) => return Err(hook_err(e)),
        }
        let state = service.get(&change).await.map_err(hook_err)?;
        let body = format!(
            "Landing was returned to the owner: {reason}\n\n\
             Rebase the change onto trunk, resolve any conflicts and request landing again."
        );
        service
            .comment(
                &change,
                &rill_lander::revert::lander_principal(),
                ThreadAnchor::Change,
                body,
                None,
                None,
                Some(state.owner.clone()),
            )
            .await
            .map_err(hook_err)?;
        Ok(())
    }

    async fn dependents_of(&self, change: ChangeId) -> rill_lander::Result<Vec<ChangeId>> {
        let stack = match self.service().stack_of(&change).await {
            Ok(stack) => stack,
            Err(ReviewError::ReviewNotFound(_)) => return Ok(Vec::new()),
            Err(e) => return Err(hook_err(e)),
        };
        Ok(stack
            .iter()
            .position(|c| *c == change)
            .map(|pos| stack[pos + 1..].to_vec())
            .unwrap_or_default())
    }
}
