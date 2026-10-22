//! Flake scores: how often a test's outcome changes when its inputs do
//! not, decaying so old flakes are forgotten.
//!
//! Each run on inputs the test already ran on is evidence: a different
//! verdict is a flip. The score is the posterior mean of a Beta prior on
//! the flip rate, `(flips + α) / (repeats + α + β)`, with counts halved
//! every [`HALF_LIFE_MS`], so a test needs repeated, recent flips to score
//! high, and a test that stops flipping drifts back to the prior.

use serde::{Deserialize, Serialize};

use super::outcome::TestOutcome;

/// Counts lose half their weight in this long: a week.
pub const HALF_LIFE_MS: f64 = 7.0 * 24.0 * 3600.0 * 1000.0;
/// Prior flips: a new test is presumed stable.
const ALPHA: f64 = 0.1;
/// Prior repeats without a flip.
const BETA: f64 = 5.0;

/// A target's decayed counts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct FlakeStats {
    /// Runs with a verdict.
    pub runs: f64,
    pub failures: f64,
    /// Runs on inputs seen before, the evidence for flipping.
    pub repeats: f64,
    /// Repeats whose verdict differed, and passes after the runner's own
    /// retries.
    pub flips: f64,
    pub updated_at_ms: i64,
    pub last: Option<TestOutcome>,
}

impl FlakeStats {
    /// Fold in a run at `at_ms`. `previous` is the verdict of the last run
    /// on the same inputs, if any.
    pub fn observe(
        &mut self,
        outcome: TestOutcome,
        retried: bool,
        previous: Option<TestOutcome>,
        at_ms: i64,
    ) {
        self.decay_to(at_ms);
        self.last = Some(outcome);
        if !outcome.is_verdict() {
            return;
        }
        self.runs += 1.0;
        if outcome == TestOutcome::Failed {
            self.failures += 1.0;
        }
        if let Some(previous) = previous.filter(|p| p.is_verdict()) {
            self.repeats += 1.0;
            if previous != outcome {
                self.flips += 1.0;
            }
        }
        if retried {
            self.repeats += 1.0;
            self.flips += 1.0;
        }
    }

    /// The flake score, 0 to 1, at `now_ms`.
    pub fn score(&self, now_ms: i64) -> f64 {
        let mut at_now = *self;
        at_now.decay_to(now_ms);
        (at_now.flips + ALPHA) / (at_now.repeats + ALPHA + BETA)
    }

    fn decay_to(&mut self, at_ms: i64) {
        let elapsed = (at_ms - self.updated_at_ms).max(0) as f64;
        if self.updated_at_ms != 0 && elapsed > 0.0 {
            let keep = 0.5f64.powf(elapsed / HALF_LIFE_MS);
            self.runs *= keep;
            self.failures *= keep;
            self.repeats *= keep;
            self.flips *= keep;
        }
        self.updated_at_ms = self.updated_at_ms.max(at_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TestOutcome::{Failed, Passed};

    const DAY: i64 = 24 * 3600 * 1000;

    #[test]
    fn a_stable_test_stays_near_zero_and_a_flipping_one_rises() {
        let (mut stable, mut flaky) = (FlakeStats::default(), FlakeStats::default());
        let mut previous = None;
        for i in 0..20 {
            stable.observe(Passed, false, Some(Passed), i);
            let outcome = if i % 3 == 0 { Failed } else { Passed };
            flaky.observe(outcome, false, previous, i);
            previous = Some(outcome);
        }
        assert!(stable.score(20) < 0.01, "{}", stable.score(20));
        assert!(flaky.score(20) > 0.4, "{}", flaky.score(20));
    }

    #[test]
    fn failing_on_new_inputs_is_not_flaky() {
        let mut broken = FlakeStats::default();
        for _ in 0..10 {
            broken.observe(Failed, false, None, 1);
        }
        assert!(broken.score(10) < 0.05);
        assert_eq!(broken.failures, 10.0);
    }

    #[test]
    fn old_flips_fade() {
        let mut stats = FlakeStats::default();
        for i in 0..6 {
            let outcome = if i % 2 == 0 { Failed } else { Passed };
            let previous = if i % 2 == 0 { Passed } else { Failed };
            stats.observe(outcome, false, Some(previous), 1 + i);
        }
        let fresh = stats.score(10);
        assert!(stats.score(10 + 28 * DAY) < fresh / 2.0);
    }

    #[test]
    fn a_pass_after_retries_counts_as_a_flip() {
        let mut stats = FlakeStats::default();
        stats.observe(Passed, true, None, 1);
        stats.observe(Passed, true, None, 2);
        assert!(stats.score(2) > 0.2);
    }
}
