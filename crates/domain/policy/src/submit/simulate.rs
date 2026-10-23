//! Replaying recorded decisions under another policy: which landings it
//! would have blocked, and how many changes would still land on agent
//! review alone.

use rill_types::Principal;
use serde::{Deserialize, Serialize};

use super::facts::{SubmitFacts, VoteSummary};
use super::policy::{SubmitDecision, SubmitPolicy};
use crate::error::Result;
use crate::risk::ChangeFacts;

/// One decision as recorded when a change landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedDecision {
    /// Who asked to land it.
    pub principal: Principal,
    pub change: ChangeFacts,
    pub review: SubmitFacts,
    pub decision: SubmitDecision,
}

/// The facts without any human vote: whether policy would let a change
/// land on agents (and approved intents) alone.
pub fn without_humans(review: &SubmitFacts) -> SubmitFacts {
    let mut out = review.clone();
    for summary in out.labels.values_mut() {
        *summary = VoteSummary {
            max: summary.agent_max,
            min: summary.min.min(summary.agent_max),
            agent_max: summary.agent_max,
            agent_votes: summary.agent_votes,
            agent_approvals: summary.agent_approvals,
            ..VoteSummary::default()
        };
    }
    out
}

/// Whether `policy` lets the change land without any human vote.
pub fn agent_only(policy: &SubmitPolicy, record: &RecordedDecision) -> Result<bool> {
    Ok(policy
        .evaluate(
            &record.principal,
            &record.change,
            &without_humans(&record.review),
        )?
        .allowed)
}

/// How one recorded decision fares under a candidate policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replayed {
    pub before: bool,
    pub after: SubmitDecision,
    /// Under the candidate, the change could have landed with no human.
    pub agent_only_after: bool,
}

/// Replay `record` under `candidate`.
pub fn replay(candidate: &SubmitPolicy, record: &RecordedDecision) -> Result<Replayed> {
    Ok(Replayed {
        before: record.decision.allowed,
        after: candidate.evaluate(&record.principal, &record.change, &record.review)?,
        agent_only_after: agent_only(candidate, record)?,
    })
}

#[cfg(test)]
mod tests {
    use rill_types::{RepoPathBuf, RiskClass};

    use super::*;

    #[test]
    fn a_stricter_policy_blocks_what_the_old_one_let_through() {
        let change = ChangeFacts::new(
            Principal::agent("coder"),
            vec![crate::risk::PathFacts::modified(
                RepoPathBuf::new("a.rs").unwrap(),
            )],
        );
        let review = SubmitFacts {
            independent_agent_approvals: 2,
            ..SubmitFacts::new(RiskClass::Low)
                .with_label(
                    "Verified",
                    VoteSummary {
                        max: 1,
                        agent_max: 1,
                        ..VoteSummary::default()
                    },
                )
                .with_label(
                    "Review",
                    VoteSummary {
                        max: 2,
                        human_max: 2,
                        human_votes: 1,
                        human_approvals: 1,
                        ..VoteSummary::default()
                    },
                )
        };
        let old = SubmitPolicy::default_rules();
        let record = RecordedDecision {
            principal: Principal::human("ann"),
            decision: old
                .evaluate(&Principal::human("ann"), &change, &review)
                .unwrap(),
            change,
            review,
        };
        assert!(record.decision.allowed);
        assert!(
            agent_only(&old, &record).unwrap(),
            "two independent agents suffice for low"
        );
        let strict = SubmitPolicy::from_source(
            r#"@id("humans_only") permit(principal, action, resource) when {
                resource.hasTag("Review") && resource.getTag("Review").human_max >= 2 };"#,
        )
        .unwrap();
        let r = replay(&strict, &record).unwrap();
        assert!(r.before && r.after.allowed && !r.agent_only_after);
    }
}
