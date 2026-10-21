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
];

/// The known issue behind `invariant`, if any.
pub fn known(invariant: Invariant) -> Option<&'static KnownIssue> {
    KNOWN_ISSUES.iter().find(|k| k.invariant == invariant)
}
