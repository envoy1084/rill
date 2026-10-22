//! Swarm-thrash detection: when most rebases in a domain conflict, more
//! requests on it only rebase each other's work away. Such a domain is
//! throttled to one request per batch until its conflict rate falls.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::rules::ThrottleRules;

/// Recent rebase outcomes per domain.
#[derive(Default)]
pub struct Thrash {
    outcomes: Mutex<BTreeMap<String, VecDeque<(Instant, bool)>>>,
}

impl Thrash {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one rebase in `domains`, and whether it conflicted.
    pub fn record(&self, domains: &BTreeSet<String>, conflicted: bool) {
        let now = Instant::now();
        let mut outcomes = self.outcomes.lock();
        for domain in domains {
            let q = outcomes.entry(domain.clone()).or_default();
            q.push_back((now, conflicted));
            while q.len() > 10_000 {
                q.pop_front();
            }
        }
    }

    /// Each domain's conflict rate over `window`, with its attempts.
    pub fn rates(&self, window: Duration) -> BTreeMap<String, (f64, u32)> {
        let mut outcomes = self.outcomes.lock();
        let now = Instant::now();
        outcomes.retain(|_, q| {
            while q
                .front()
                .is_some_and(|(t, _)| now.duration_since(*t) > window)
            {
                q.pop_front();
            }
            !q.is_empty()
        });
        outcomes
            .iter()
            .map(|(d, q)| {
                let conflicts = q.iter().filter(|(_, c)| *c).count();
                (
                    d.clone(),
                    (conflicts as f64 / q.len() as f64, q.len() as u32),
                )
            })
            .collect()
    }

    /// The domains throttled under `rules`.
    pub fn throttled(&self, rules: &ThrottleRules) -> BTreeSet<String> {
        self.rates(Duration::from_secs(rules.window_secs))
            .into_iter()
            .filter(|(_, (rate, attempts))| {
                *attempts >= rules.min_attempts && *rate >= rules.conflict_rate
            })
            .map(|(d, _)| d)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_domain_is_throttled_once_most_of_enough_attempts_conflict() {
        let thrash = Thrash::new();
        let rules = ThrottleRules {
            conflict_rate: 0.5,
            min_attempts: 4,
            window_secs: 60,
        };
        let hot: BTreeSet<String> = ["hot/".to_string()].into();
        for conflicted in [true, true, false] {
            thrash.record(&hot, conflicted);
        }
        assert!(thrash.throttled(&rules).is_empty(), "too few attempts");
        thrash.record(&hot, false);
        assert_eq!(thrash.throttled(&rules), hot);
        thrash.record(&hot, false);
        assert!(thrash.throttled(&rules).is_empty(), "the rate fell");
    }
}
