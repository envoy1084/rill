//! Landing policy.

use std::time::Duration;

use rill_types::RiskClass;
use serde::{Deserialize, Serialize};

pub use rill_types::VerifyMode;

/// Verification mode per risk class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VerifyModes {
    pub trivial: VerifyMode,
    pub low: VerifyMode,
    pub normal: VerifyMode,
    pub sensitive: VerifyMode,
    pub critical: VerifyMode,
}

impl Default for VerifyModes {
    fn default() -> Self {
        VerifyModes {
            trivial: VerifyMode::None,
            low: VerifyMode::Affected,
            normal: VerifyMode::Affected,
            sensitive: VerifyMode::Full,
            critical: VerifyMode::Full,
        }
    }
}

impl VerifyModes {
    /// The mode for `risk`.
    pub fn for_risk(&self, risk: RiskClass) -> VerifyMode {
        match risk {
            RiskClass::Trivial => self.trivial,
            RiskClass::Low => self.low,
            RiskClass::Normal => self.normal,
            RiskClass::Sensitive => self.sensitive,
            RiskClass::Critical => self.critical,
        }
    }
}

/// Tunables of the landing pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LandPolicy {
    /// Pre-land verification per risk class.
    pub modes: VerifyModes,
    /// Longest a pre-land verification may take before the request fails.
    pub max_wait: Duration,
    /// Longest pause between queue sweeps in `run_forever`; a new request
    /// starts a sweep at once.
    pub batch_window: Duration,
    /// Revert a culprit automatically once post-land bisection finds it,
    /// unless a forward fix lands within `revert_grace`. On by default.
    pub auto_revert: bool,
    /// How long to wait for a forward fix before an automatic revert.
    pub revert_grace: Duration,
    /// How many times to retry the trunk compare-and-swap after another
    /// landing wins the race.
    pub max_retries: u32,
    /// Unfinished requests one principal may have queued; more are refused
    /// until some finish. Reverts are exempt.
    pub max_in_flight: usize,
    /// Unfinished requests charged to one sponsor (all its agents).
    pub max_sponsor_in_flight: usize,
    /// Unfinished requests in all; past it new ones are refused with a
    /// time to retry.
    pub max_queued: usize,
    /// Shares of the agent lane by sponsor; 1 when unlisted.
    pub sponsor_weights: std::collections::BTreeMap<String, u32>,
    /// Red this long makes trunk guarded: landings touching what fails
    /// wait.
    pub guard_after: Duration,
    /// This many open incidents at once make trunk guarded.
    pub guard_incidents: usize,
}

impl Default for LandPolicy {
    fn default() -> Self {
        LandPolicy {
            modes: VerifyModes::default(),
            max_wait: Duration::from_secs(30 * 60),
            batch_window: Duration::from_secs(2),
            auto_revert: true,
            revert_grace: Duration::from_secs(15 * 60),
            max_retries: 8,
            max_in_flight: 50,
            max_sponsor_in_flight: 2_000,
            max_queued: 10_000,
            sponsor_weights: Default::default(),
            guard_after: Duration::from_secs(30 * 60),
            guard_incidents: 3,
        }
    }
}

impl LandPolicy {
    /// Verification mode for a request of `risk`.
    pub fn verify_mode(&self, risk: RiskClass) -> VerifyMode {
        self.modes.for_risk(risk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_sensible_and_serializable() {
        let policy = LandPolicy::default();
        assert_eq!(policy.verify_mode(RiskClass::Trivial), VerifyMode::None);
        assert_eq!(policy.verify_mode(RiskClass::Critical), VerifyMode::Full);
        let bytes = postcard::to_allocvec(&policy).unwrap();
        let back: LandPolicy = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back, policy);
    }
}
