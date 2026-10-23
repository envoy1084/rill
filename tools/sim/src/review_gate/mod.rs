//! The phase 23 gate: review at scale. Changes by tens of thousands of
//! agents (sponsored by a thousand people) are reviewed by an agent fleet
//! under the default policy, with defects seeded into some of them. The
//! gate holds when at most a tenth of changes need a person and at least
//! nine in ten seeded defects are caught, before landing by reviewers or
//! after it by sampled audits.

mod reviewing;
mod world;

use rill_review_runner::backend::Fake;

use crate::rng::SimRng;
pub use world::GateWorld;

/// The workload.
#[derive(Debug, Clone, Copy)]
pub struct GateConfig {
    pub seed: u64,
    pub humans: usize,
    pub agents: usize,
    pub changes: usize,
    /// Changes outside every approved intent's scope, per thousand.
    pub out_of_scope_permille: u32,
    /// Changes to repository configuration (critical risk), per thousand.
    pub sensitive_permille: u32,
    /// Changes carrying a seeded defect, per thousand.
    pub defect_permille: u32,
    /// What the fleet's fake reviewers catch and wrongly block.
    pub detect_permille: u32,
    pub false_positive_permille: u32,
    /// Agent-only landings audited, per thousand (the intents' rate).
    pub audit_permille: u32,
}

impl Default for GateConfig {
    fn default() -> Self {
        GateConfig {
            seed: 23,
            humans: 1_000,
            agents: 30_000,
            changes: 1_500,
            out_of_scope_permille: 60,
            sensitive_permille: 20,
            defect_permille: 100,
            detect_permille: 900,
            false_positive_permille: 20,
            audit_permille: 50,
        }
    }
}

/// What the run found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GateReport {
    pub changes: usize,
    /// Changes the policy let land on agent review (and approved intents).
    pub agent_only: usize,
    /// Changes that needed a person's vote to land.
    pub needed_human: usize,
    /// Changes sent back to their author by a blocking finding.
    pub blocked: usize,
    pub defects: usize,
    pub caught_by_reviewers: usize,
    pub caught_by_audits: usize,
    /// Seeded defects that landed unseen.
    pub escaped: usize,
}

impl GateReport {
    /// Changes that needed a person, per thousand.
    pub fn human_permille(&self) -> usize {
        self.needed_human * 1000 / self.changes.max(1)
    }

    /// Seeded defects caught, per thousand.
    pub fn caught_permille(&self) -> usize {
        (self.caught_by_reviewers + self.caught_by_audits) * 1000 / self.defects.max(1)
    }

    /// Whether the phase gate holds: at most 10% to humans, at least 90%
    /// of seeded defects caught.
    pub fn passes(&self) -> bool {
        self.human_permille() <= 100 && self.caught_permille() >= 900
    }
}

/// Run the gate workload.
pub async fn run_gate(config: GateConfig) -> GateReport {
    let mut rng = SimRng::new(config.seed);
    let world = GateWorld::new(&config, &mut rng).await;
    let fleet = Fake {
        detect_permille: config.detect_permille,
        false_positive_permille: config.false_positive_permille,
    };
    let mut report = GateReport {
        changes: config.changes,
        ..GateReport::default()
    };
    for n in 0..config.changes {
        let change = world.write_change(&config, &mut rng, n).await;
        world.review(&fleet, &change, &mut report).await;
    }
    report
}
