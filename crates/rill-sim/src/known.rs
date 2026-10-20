//! Invariant violations that the v1 code is known to produce, and the
//! roadmap phase whose redesign removes each. A violation of any other
//! invariant is a new bug.

use crate::oracle::Invariant;

/// A violation the current code is expected to produce under faults.
#[derive(Debug, Clone, Copy)]
pub struct KnownIssue {
    pub invariant: Invariant,
    pub cause: &'static str,
    pub fixed_by_phase: u8,
}

/// Every known issue.
pub const KNOWN_ISSUES: &[KnownIssue] = &[
    KnownIssue {
        invariant: Invariant::LandRequestsResolve,
        cause: "the land queue and its tickets live in process memory, so a restart forgets queued requests",
        fixed_by_phase: 9,
    },
    KnownIssue {
        invariant: Invariant::ReviewMatchesTrunk,
        cause: "the trunk write, the landing record and the review status are separate writes; a failure between them leaves the review out of step with trunk",
        fixed_by_phase: 6,
    },
    KnownIssue {
        invariant: Invariant::LandsOnce,
        cause: "a trunk write whose acknowledgement is lost counts as a failure, so the change returns to its owner and can land again",
        fixed_by_phase: 6,
    },
];

/// The known issue behind `invariant`, if any.
pub fn known(invariant: Invariant) -> Option<&'static KnownIssue> {
    KNOWN_ISSUES.iter().find(|k| k.invariant == invariant)
}
