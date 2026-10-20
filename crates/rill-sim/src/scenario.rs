//! Knobs for one kind of run.

/// What a run does and how hostile its environment is.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: &'static str,
    /// Simulated agents; two human reviewers are always added.
    pub agents: usize,
    pub steps: usize,
    /// Probability a store write fails outright.
    pub fail_rate: f64,
    /// Probability a store write commits but reports failure.
    pub lose_ack_rate: f64,
    /// Probability an object read returns corrupted bytes.
    pub corrupt_rate: f64,
    /// Probability a step is a process restart.
    pub restart_rate: f64,
    /// Share of edits that touch the one shared hot file.
    pub hot_file_share: f64,
    /// Share of new changes based on an older trunk position.
    pub stale_base_share: f64,
}

impl Scenario {
    /// No faults or restarts: every invariant must hold.
    pub fn clean() -> Self {
        Scenario {
            name: "clean",
            agents: 6,
            steps: 160,
            fail_rate: 0.0,
            lose_ack_rate: 0.0,
            corrupt_rate: 0.0,
            restart_rate: 0.0,
            hot_file_share: 0.3,
            stale_base_share: 0.2,
        }
    }

    /// Store faults and restarts: violations must all be known issues.
    pub fn faulty() -> Self {
        Scenario {
            name: "faulty",
            fail_rate: 0.03,
            lose_ack_rate: 0.02,
            corrupt_rate: 0.01,
            restart_rate: 0.02,
            ..Scenario::clean()
        }
    }

    /// Many agents on few files: a contention stress test.
    pub fn contended() -> Self {
        Scenario {
            name: "contended",
            agents: 16,
            steps: 400,
            hot_file_share: 0.7,
            ..Scenario::clean()
        }
    }

    /// Look a scenario up by name.
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "clean" => Some(Self::clean()),
            "faulty" => Some(Self::faulty()),
            "contended" => Some(Self::contended()),
            _ => None,
        }
    }
}
