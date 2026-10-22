//! The actions an actor takes in one step and how they end.

use crate::rng::SimRng;

/// Something an actor does in one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NewChange,
    Amend,
    Submit,
    Vote,
    RequestLand,
    Sweep,
    Sync,
    Abandon,
    DeriveWork,
    ZombieLand,
    RivalSweep,
    Partition,
    KillAgent,
}

impl Action {
    const WEIGHTED: &'static [(Action, usize)] = &[
        (Action::NewChange, 20),
        (Action::Amend, 8),
        (Action::Submit, 14),
        (Action::Vote, 20),
        (Action::RequestLand, 14),
        (Action::Sweep, 12),
        (Action::Sync, 6),
        (Action::Abandon, 3),
        (Action::DeriveWork, 8),
        (Action::ZombieLand, 3),
        (Action::RivalSweep, 6),
        (Action::Partition, 1),
        (Action::KillAgent, 1),
    ];

    /// Draw an action by weight.
    pub fn draw(rng: &mut SimRng) -> Action {
        let total: usize = Self::WEIGHTED.iter().map(|(_, w)| w).sum();
        let mut roll = rng.below(total);
        for (action, weight) in Self::WEIGHTED {
            if roll < *weight {
                return *action;
            }
            roll -= weight;
        }
        unreachable!("roll is below the total weight")
    }
}

/// How an action ended, for the run trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    Done,
    Skipped,
    Failed,
}
