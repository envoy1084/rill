//! Review-side facts that submit rules evaluate against.

use std::collections::{BTreeMap, BTreeSet};

use rill_types::objects::review::CommentCategory;
use rill_types::{LabelVote, PrincipalKind, ReviewState, RiskClass};
use serde::{Deserialize, Serialize};

/// Aggregated votes on one label for the current version of a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VoteSummary {
    /// Highest vote value across all voters; 0 when nobody voted.
    pub max: i8,
    /// Lowest vote value across all voters; 0 when nobody voted.
    pub min: i8,
    /// Highest vote cast by a human; 0 when none.
    pub human_max: i8,
    /// Highest vote cast by an agent; 0 when none.
    pub agent_max: i8,
    /// Distinct humans that voted at all.
    pub human_votes: u32,
    /// Distinct agents that voted at all.
    pub agent_votes: u32,
    /// Distinct humans that voted a positive value.
    pub human_approvals: u32,
    /// Distinct agents that voted a positive value.
    pub agent_approvals: u32,
}

impl VoteSummary {
    /// Aggregate a set of votes that all target the same label and version.
    /// A principal that voted more than once counts once, with their latest
    /// vote (the one furthest along in `votes`).
    pub fn from_votes<'a>(votes: impl IntoIterator<Item = &'a LabelVote>) -> Self {
        let mut latest: BTreeMap<_, &LabelVote> = BTreeMap::new();
        for v in votes {
            latest.insert(v.voter.id, v);
        }
        // Maxima are folded through `Option` so that a lone negative vote
        // reports as negative rather than being hidden by the 0 default.
        let fold = |acc: Option<i8>, v: i8| Some(acc.map_or(v, |a| a.max(v)));
        let (mut max, mut min, mut human_max, mut agent_max) = (None, None, None, None);
        let mut s = VoteSummary::default();
        for v in latest.values() {
            max = fold(max, v.value);
            min = Some(min.map_or(v.value, |m: i8| m.min(v.value)));
            match v.voter.kind {
                PrincipalKind::Human => {
                    human_max = fold(human_max, v.value);
                    s.human_votes += 1;
                    if v.value > 0 {
                        s.human_approvals += 1;
                    }
                }
                PrincipalKind::Agent => {
                    agent_max = fold(agent_max, v.value);
                    s.agent_votes += 1;
                    if v.value > 0 {
                        s.agent_approvals += 1;
                    }
                }
                // Service votes (CI, bots) count toward max/min only; the
                // human/agent breakdown is what rules reason about.
                PrincipalKind::Service => {}
            }
        }
        s.max = max.unwrap_or(0);
        s.min = min.unwrap_or(0);
        s.human_max = human_max.unwrap_or(0);
        s.agent_max = agent_max.unwrap_or(0);
        s
    }
}

/// What the review state contributes to a submit decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitFacts {
    /// Risk class of the current version.
    pub risk: RiskClass,
    /// Vote summaries keyed by label name.
    pub labels: BTreeMap<String, VoteSummary>,
    /// Unresolved threads of any category.
    pub unresolved_threads: u32,
    /// Unresolved threads whose category is `Bug` or `Security`.
    pub unresolved_blocking_threads: u32,
    /// Whether a sensitive-path glob fired in the risk assessment.
    pub touches_sensitive: bool,
}

impl SubmitFacts {
    /// Facts with no votes and no threads at the given risk.
    pub fn new(risk: RiskClass) -> Self {
        SubmitFacts {
            risk,
            labels: BTreeMap::new(),
            unresolved_threads: 0,
            unresolved_blocking_threads: 0,
            touches_sensitive: false,
        }
    }

    /// Derive facts from a review state, counting only votes on the current
    /// version.
    pub fn from_review(state: &ReviewState) -> Self {
        let mut by_label: BTreeMap<&str, Vec<&LabelVote>> = BTreeMap::new();
        for v in state.current_votes() {
            by_label.entry(v.label.as_str()).or_default().push(v);
        }
        let labels = by_label
            .into_iter()
            .map(|(name, votes)| (name.to_string(), VoteSummary::from_votes(votes)))
            .collect();
        let unresolved: Vec<_> = state.threads.iter().filter(|t| !t.resolved).collect();
        let blocking = unresolved
            .iter()
            .filter(|t| {
                t.comments.iter().any(|c| {
                    matches!(
                        c.category,
                        Some(CommentCategory::Bug | CommentCategory::Security)
                    )
                })
            })
            .count();
        SubmitFacts {
            risk: state.risk,
            labels,
            unresolved_threads: unresolved.len() as u32,
            unresolved_blocking_threads: blocking as u32,
            touches_sensitive: crate::risk::touches_sensitive(&state.risk_reasons),
        }
    }

    /// Set a label's summary.
    pub fn with_label(mut self, name: &str, summary: VoteSummary) -> Self {
        self.labels.insert(name.to_string(), summary);
        self
    }

    /// Set the thread counts.
    pub fn with_threads(mut self, unresolved: u32, blocking: u32) -> Self {
        self.unresolved_threads = unresolved;
        self.unresolved_blocking_threads = blocking;
        self
    }

    /// Names of the labels that have at least one vote.
    pub fn label_names(&self) -> BTreeSet<&str> {
        self.labels.keys().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{ObjectHash, Principal, Timestamp};

    fn vote(label: &str, value: i8, voter: Principal) -> LabelVote {
        LabelVote {
            label: label.into(),
            value,
            voter,
            version: ObjectHash::ZERO,
            voted_at: Timestamp::from_millis(0),
            reason: None,
        }
    }

    #[test]
    fn summary_counts_distinct_voters_with_latest_vote() {
        let alice = Principal::human("alice");
        let bot = Principal::agent("bot");
        let votes = vec![
            vote("Review", 2, alice.clone()),
            vote("Review", -1, alice.clone()),
            vote("Review", 1, bot.clone()),
            vote("Review", 1, Principal::agent("bot2")),
        ];
        let s = VoteSummary::from_votes(&votes);
        assert_eq!(s.max, 1);
        assert_eq!(s.min, -1);
        assert_eq!(s.human_max, -1);
        assert_eq!(s.human_votes, 1);
        assert_eq!(s.human_approvals, 0);
        assert_eq!(s.agent_max, 1);
        assert_eq!(s.agent_votes, 2);
        assert_eq!(s.agent_approvals, 2);
    }
}
