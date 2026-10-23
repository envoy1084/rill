//! Code owners: the owner sets covering a path, and a change's standing
//! with each set it touches.

use std::time::Duration;

use rill_proto::rill::v1 as pb;

/// A group of principals of which `required` must approve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerSet {
    /// `<dir>#default` or `<dir>#rule<n>`.
    pub id: String,
    /// The directory declaring it; empty for the root.
    pub dir: String,
    /// `user:<name>`, `agent:<name>` or `team:<name>`.
    pub owners: Vec<String>,
    /// Approvals needed from `owners`.
    pub required: u32,
    /// Touching it makes a change at least sensitive.
    pub sensitive: bool,
    /// How long a required review may wait before escalating.
    pub sla: Option<Duration>,
    /// Who an overdue review escalates to.
    pub escalate_to: Option<String>,
}

impl From<pb::OwnerSet> for OwnerSet {
    fn from(s: pb::OwnerSet) -> Self {
        OwnerSet {
            id: s.id,
            dir: s.dir,
            owners: s.owners,
            required: s.required,
            sensitive: s.sensitive,
            sla: s.sla_ms.map(|ms| Duration::from_millis(ms.max(0) as u64)),
            escalate_to: s.escalate_to,
        }
    }
}

/// One owner set's state for a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerSetStatus {
    /// The set.
    pub set: OwnerSet,
    /// Distinct approvers who count for the set.
    pub approvals: u32,
    /// It has its required approvals.
    pub covered: bool,
    /// Where an overdue review was last escalated to.
    pub escalated_to: Option<String>,
    /// How many times it was escalated.
    pub escalations: u32,
}

/// Every owner set a change touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ownership {
    /// Each set, in id order.
    pub sets: Vec<OwnerSetStatus>,
    /// Every set is covered (true when the change touches none).
    pub covered: bool,
}

impl From<pb::ExplainOwnershipResponse> for Ownership {
    fn from(r: pb::ExplainOwnershipResponse) -> Self {
        Ownership {
            covered: r.covered,
            sets: r
                .sets
                .into_iter()
                .map(|s| OwnerSetStatus {
                    set: s.set.into_option().unwrap_or_default().into(),
                    approvals: s.approvals,
                    covered: s.covered,
                    escalated_to: s.escalated_to,
                    escalations: s.escalations,
                })
                .collect(),
        }
    }
}
