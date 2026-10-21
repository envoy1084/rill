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
    /// Pause between queue sweeps in `run_forever`; requests arriving
    /// within one window are batched together.
    pub batch_window: Duration,
    /// Revert a culprit automatically once post-land bisection finds it.
    pub auto_revert: bool,
    /// How long to wait for a forward fix before an automatic revert.
    pub revert_grace: Duration,
    /// How many times to retry the trunk compare-and-swap after another
    /// landing wins the race.
    pub max_retries: u32,
}

impl Default for LandPolicy {
    fn default() -> Self {
        LandPolicy {
            modes: VerifyModes::default(),
            max_wait: Duration::from_secs(30 * 60),
            batch_window: Duration::from_secs(2),
            auto_revert: false,
            revert_grace: Duration::from_secs(15 * 60),
            max_retries: 8,
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
