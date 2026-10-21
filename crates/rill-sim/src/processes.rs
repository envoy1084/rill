//! The rival server process and network partitions: two landers compete
//! for one lease, and either may lose the metadata store for longer than
//! the lease lasts, so the lease changes hands while the old holder still
//! believes it has it.

use std::sync::Arc;

use crate::actors::{Ctx, Outcome};
use crate::world::PROCESSES;

/// Steps a partition lasts at least and at most; a step is a simulated
/// second and the lease lasts ten.
const PARTITION_STEPS: (usize, usize) = (12, 20);

impl Ctx<'_> {
    /// The rival process serves the land queue, if it holds the lease or
    /// can take it.
    pub(crate) async fn rival_sweep(&mut self) -> Outcome {
        let rival = Arc::clone(&self.world.rival);
        let outcome = match rival.run_once().await {
            Ok(landed) => {
                self.oracle.rival_landings += landed;
                Outcome::Done
            }
            Err(_) => Outcome::Failed,
        };
        self.oracle.observe_trunk(&self.world.repo).await;
        outcome
    }

    /// Cut a process off from the metadata store for a while.
    pub(crate) async fn partition(&mut self) -> Outcome {
        let process = self.rng.below(PROCESSES);
        if !self.partitions || self.world.is_partitioned(process) {
            return Outcome::Skipped;
        }
        let (least, most) = PARTITION_STEPS;
        let steps = least + self.rng.below(most - least + 1);
        self.world.partition(process, self.step + steps);
        self.oracle.partitions += 1;
        Outcome::Done
    }
}
