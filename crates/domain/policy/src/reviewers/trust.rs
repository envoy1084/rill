//! A reviewer's track record and the trust it earns.
//!
//! Outcomes come from what happens after review: an approved change that
//! lands is a good call until it is reverted, blamed for a red trunk or
//! fails a human audit (an escape); a blocking finding is a good call when
//! the author changed the code before resolving it, and a false alarm when
//! the thread was resolved on the version it was raised on. Trust is the
//! mean of the Beta posterior over good and bad calls, starting from one
//! of each; a reviewer whose trust falls below the line after enough
//! outcomes is demoted until an owner reinstates it.

use rill_types::Timestamp;
use serde::{Deserialize, Serialize};

/// Outcomes before a reviewer can be demoted.
pub const MIN_OUTCOMES: u64 = 20;
/// Trust, in permille, below which a reviewer is demoted.
pub const DEMOTE_BELOW: u16 = 700;

/// One reviewer's record in a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewerStats {
    /// Changes it approved that landed.
    pub approvals: u64,
    /// Of those, ones later reverted, blamed or failed in audit.
    pub escapes: u64,
    /// Blocking findings the author fixed.
    pub blocks: u64,
    /// Blocking findings resolved without a change.
    pub dismissed: u64,
    /// Demoted: routed nothing, and its approvals do not count.
    pub demoted: bool,
    /// Why and when it was demoted or reinstated last.
    pub note: String,
    pub since: Option<Timestamp>,
}

impl ReviewerStats {
    fn good(&self) -> u64 {
        self.approvals.saturating_sub(self.escapes) + self.blocks
    }

    fn bad(&self) -> u64 {
        self.escapes + self.dismissed
    }

    /// Outcomes counted.
    pub fn outcomes(&self) -> u64 {
        self.good() + self.bad()
    }

    /// Trust in permille: the Beta(good + 1, bad + 1) mean.
    pub fn trust(&self) -> u16 {
        let (good, bad) = (self.good() as f64, self.bad() as f64);
        ((good + 1.0) / (good + bad + 2.0) * 1000.0).round() as u16
    }

    /// Demote it when it has enough outcomes and too little trust;
    /// whether it was demoted now.
    pub fn check(&mut self, now: Timestamp) -> bool {
        if self.demoted || self.outcomes() < MIN_OUTCOMES || self.trust() >= DEMOTE_BELOW {
            return false;
        }
        self.demoted = true;
        self.note = format!(
            "demoted: trust {} below {DEMOTE_BELOW} after {} outcomes",
            self.trust(),
            self.outcomes()
        );
        self.since = Some(now);
        true
    }

    /// Start over: counts cleared, no longer demoted.
    pub fn reinstate(&mut self, by: &str, now: Timestamp) {
        *self = ReviewerStats {
            note: format!("reinstated by {by}"),
            since: Some(now),
            ..ReviewerStats::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_follows_outcomes_and_demotes_only_with_enough_of_them() {
        let now = Timestamp::from_millis(0);
        let fresh = ReviewerStats::default();
        assert_eq!(fresh.trust(), 500);
        let mut s = ReviewerStats {
            approvals: 10,
            escapes: 6,
            ..ReviewerStats::default()
        };
        assert!(!s.check(now), "too few outcomes");
        s.approvals = 30;
        s.escapes = 15;
        s.dismissed = 5;
        assert!(s.trust() < DEMOTE_BELOW);
        assert!(s.check(now));
        assert!(!s.check(now), "demoted once");
        s.reinstate("ann", now);
        assert!(!s.demoted && s.outcomes() == 0);

        let good = ReviewerStats {
            approvals: 100,
            escapes: 2,
            blocks: 10,
            dismissed: 3,
            ..ReviewerStats::default()
        };
        assert!(good.trust() > 940, "{}", good.trust());
    }
}
