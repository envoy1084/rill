//! Review and landing: submitting, voting, requesting a land and sweeping
//! the queue.

use std::sync::Arc;

use rill_lander::LandRequest;
use rill_types::ReviewStatus;

use super::{Agent, Ctx, Outcome};

impl Ctx<'_> {
    pub(super) async fn submit(&mut self, agent: &mut Agent) -> Outcome {
        let Some(owned) = self.pick_own(agent, Some(false)) else {
            return Outcome::Skipped;
        };
        let submitted = self
            .world
            .review()
            .submit(&owned.id, &agent.principal)
            .await;
        // A lost acknowledgement may have stored the review anyway.
        let exists = matches!(self.world.repo.get_review(&owned.id).await, Ok(Some(_)));
        if submitted.is_ok() || exists {
            if let Some(c) = agent.changes.iter_mut().find(|c| c.id == owned.id) {
                c.submitted = true;
            }
        }
        if submitted.is_ok() {
            Outcome::Done
        } else {
            Outcome::Failed
        }
    }

    pub(super) async fn vote(&mut self, agent: &mut Agent) -> Outcome {
        let Some(owned) = self.pick_own(agent, Some(true)) else {
            return Outcome::Skipped;
        };
        let Some(voter) = self.rng.pick(self.reviewers).cloned() else {
            return Outcome::Skipped;
        };
        let (label, value) = match self.rng.below(20) {
            0 => ("Review", -2),
            1..=4 => ("Verified", 1),
            _ => ("Review", 2),
        };
        match self
            .world
            .review()
            .vote(&owned.id, &voter, label, value, None)
            .await
        {
            Ok(_) => Outcome::Done,
            Err(_) => Outcome::Failed,
        }
    }

    pub(super) async fn request_land(&mut self, agent: &mut Agent) -> Outcome {
        let Some(owned) = self.pick_own(agent, Some(true)) else {
            return Outcome::Skipped;
        };
        let review = self.world.review();
        let Ok(state) = review.get(&owned.id).await else {
            return Outcome::Failed;
        };
        if state.status != ReviewStatus::Landable {
            return Outcome::Skipped;
        }
        let Ok(decision) = review.evaluate_for(&state, &agent.principal).await else {
            return Outcome::Failed;
        };
        let Some(version) = state.current_version() else {
            return Outcome::Skipped;
        };
        if !decision.allowed {
            return Outcome::Skipped;
        }
        let request = LandRequest::new(owned.id, version, agent.principal.clone(), state.risk);
        match self.world.lander.enqueue(request).await {
            Ok(ticket) => {
                self.oracle.requested(owned.id, ticket);
                Outcome::Done
            }
            Err(_) => Outcome::Failed,
        }
    }

    pub(super) async fn sweep(&mut self) -> Outcome {
        let lander = Arc::clone(&self.world.lander);
        let outcome = match lander.run_once().await {
            Ok(_) => Outcome::Done,
            Err(_) => Outcome::Failed,
        };
        self.oracle.observe_trunk(&self.world.repo).await;
        outcome
    }
}
