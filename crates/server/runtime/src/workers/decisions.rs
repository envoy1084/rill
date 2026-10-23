//! Recording the decision behind every landing: the policy the landing
//! was checked against (by the digest the landing carries), re-evaluated
//! on the change's facts as they were when it landed.

use std::sync::Arc;

use async_trait::async_trait;
use rill_events::EventFilter;
use rill_policy::submit::simulate::RecordedDecision;
use rill_policy::{SubmitFacts, SubmitPolicy};
use rill_types::{Event, EventKind};

use super::consumer::Consumer;
use crate::error::Result;
use crate::state::decisions::{self, LandingDecision};
use crate::state::policy::policy_text;
use crate::state::RepoContext;

/// Records landing decisions.
pub struct Recorder;

#[async_trait]
impl Consumer for Recorder {
    fn name(&self) -> &'static str {
        "landing-decisions"
    }

    fn filter(&self) -> EventFilter {
        EventFilter::kinds(["landed"])
    }

    async fn handle(&self, ctx: &Arc<RepoContext>, event: &Event) -> Result<()> {
        let EventKind::Landed {
            seq,
            change,
            version,
        } = &event.kind
        else {
            return Ok(());
        };
        let Some(landing) = ctx.repo.landing(*seq).await? else {
            return Ok(());
        };
        let Some(digest) = landing.policy else {
            return Ok(());
        };
        let Some(state) = ctx.repo.get_review(change).await? else {
            return Ok(());
        };
        let review = ctx.review();
        let text = policy_text(&ctx.repo, &digest).await?;
        let policy = match &text {
            Some(text) => SubmitPolicy::from_source(text)?,
            None => review.policy().as_ref().clone(),
        };
        let mut change_facts = review.change_facts(&state).await?;
        change_facts.at = landing.landed_at;
        let submit = SubmitFacts::from_review(&state, &change_facts);
        let decision = policy.evaluate(&landing.requester, &change_facts, &submit)?;
        decisions::store(
            &ctx.repo,
            &LandingDecision {
                seq: *seq,
                change: *change,
                version: *version,
                policy: hex::encode(digest),
                source: review.policy_source().to_string(),
                record: RecordedDecision {
                    principal: landing.requester.clone(),
                    change: change_facts,
                    review: submit,
                    decision,
                },
            },
        )
        .await
    }
}
