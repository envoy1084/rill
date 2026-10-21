//! Target rates as a fraction of the design point D1.

/// Per-second rates at 100% of D1 (research plan, target workload).
const D1_READS_PER_SEC: f64 = 50_000.0;
const D1_VERSIONS_PER_SEC: f64 = 35.0;
const D1_VOTES_PER_SEC: f64 = 23.0;
const D1_LANDS_PER_SEC: f64 = 1.16;

/// Rates for one run.
#[derive(Debug, Clone, Copy)]
pub struct Profile {
    pub percent: f64,
    pub reads_per_sec: f64,
    pub versions_per_sec: f64,
    pub votes_per_sec: f64,
    pub lands_per_sec: f64,
}

impl Profile {
    /// `percent` of D1, e.g. `1.0` for 1%.
    pub fn of_d1(percent: f64) -> Self {
        let f = percent / 100.0;
        Profile {
            percent,
            reads_per_sec: D1_READS_PER_SEC * f,
            versions_per_sec: D1_VERSIONS_PER_SEC * f,
            votes_per_sec: D1_VOTES_PER_SEC * f,
            lands_per_sec: D1_LANDS_PER_SEC * f,
        }
    }

    /// The same rates with `lands_per_sec` landings, and enough new
    /// versions and votes to feed them.
    pub fn with_lands(mut self, lands_per_sec: f64) -> Self {
        self.lands_per_sec = lands_per_sec;
        self.versions_per_sec = self.versions_per_sec.max(lands_per_sec * 1.5);
        self.votes_per_sec = self.votes_per_sec.max(lands_per_sec * 1.5);
        self
    }
}
