//! Submit evaluation and the `Open` / `Landable` status refresh.

use rill_policy::{ChangeFacts, SubmitDecision};
use rill_types::{ChangeId, Principal, ReviewState, ReviewStatus};

use crate::error::Result;
use crate::lifecycle::service_actor;
use crate::service::ReviewService;

impl ReviewService {
    /// Evaluate the submit policy for the change's current state, as if its
    /// owner were landing it.
    pub async fn evaluate(&self, change_id: &ChangeId) -> Result<SubmitDecision> {
        let state = self.get(change_id).await?;
        let owner = state.owner.clone();
        self.evaluate_for(&state, &owner).await
    }

    /// Evaluate the submit policy for `state` with `principal` as the
    /// actor requesting the land.
    pub async fn evaluate_for(
        &self,
        state: &ReviewState,
        principal: &Principal,
    ) -> Result<SubmitDecision> {
        let facts = self.change_facts(state).await?;
        self.decide(state, &facts, principal)
    }

    /// Re-evaluate the stored state and persist any `Open` / `Landable`
    /// flip.
    pub async fn refresh(&self, change_id: &ChangeId) -> Result<ReviewState> {
        let current = self.get(change_id).await?;
        let facts = self.change_facts(&current).await?;
        let (state, transition) = self
            .update(change_id, &service_actor(), |s| {
                self.refresh_status(s, &facts)
            })
            .await?;
        self.emit_transition(change_id, transition);
        Ok(state)
    }

    /// Set the status to `Landable` when the policy allows landing and the
    /// change is `Open`, or back to `Open` when it no longer does. Other
    /// statuses are left alone. Returns the transition made, if any.
    pub fn refresh_status(
        &self,
        state: &mut ReviewState,
        facts: &ChangeFacts,
    ) -> Result<Option<(ReviewStatus, ReviewStatus)>> {
        if !matches!(state.status, ReviewStatus::Open | ReviewStatus::Landable) {
            return Ok(None);
        }
        let decision = self.decide(state, facts, &state.owner)?;
        let target = if decision.allowed {
            ReviewStatus::Landable
        } else {
            ReviewStatus::Open
        };
        if state.status == target {
            return Ok(None);
        }
        let from = state.status;
        state.status = target;
        Ok(Some((from, target)))
    }

    fn decide(
        &self,
        state: &ReviewState,
        facts: &ChangeFacts,
        principal: &Principal,
    ) -> Result<SubmitDecision> {
        let review = self.submit_facts(state, facts);
        Ok(self.policy.evaluate(principal, facts, &review)?)
    }
}
