//! A derive worker that sometimes dies holding its job, and draining the
//! queue once faults stop.

use rill_jobs::{Error as JobError, Queue};
use rill_lander::land::followups::DERIVE_QUEUE;
use rill_store::RepoStore;
use rill_types::ChangeVersionHash;

use crate::actors::{Ctx, Outcome};
use crate::oracle::{Invariant, Oracle};
use crate::world::World;

/// Share of claims whose worker dies before finishing.
const DEATH_RATE: f64 = 0.3;
/// Jobs drained at most while settling.
const DRAIN_LIMIT: usize = 10_000;
const WORKER: &str = "sim-worker";

fn queue(repo: &RepoStore) -> Queue {
    Queue::new(repo.clone(), DERIVE_QUEUE)
}

impl Ctx<'_> {
    /// Take back expired claims, then claim a derive job and either run it
    /// or die holding it.
    pub(crate) async fn derive_work(&mut self) -> Outcome {
        let queue = queue(&self.world.repo);
        let _ = queue.reap().await;
        let claim = match queue.claim(WORKER).await {
            Ok(Some(claim)) => claim,
            Ok(None) => return Outcome::Skipped,
            Err(_) => return Outcome::Failed,
        };
        if self.rng.chance(DEATH_RATE) {
            self.oracle.killed_jobs.insert(claim.job.id);
            return Outcome::Done;
        }
        match run_job(self.world, &queue, &claim).await {
            Ok(()) => Outcome::Done,
            Err(_) => Outcome::Failed,
        }
    }
}

async fn run_job(world: &World, queue: &Queue, claim: &rill_jobs::Claim) -> Result<(), JobError> {
    let derived = match ChangeVersionHash::from_slice(&claim.job.payload) {
        Ok(version) => world
            .derive
            .ensure_background(&version)
            .await
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    match derived {
        Ok(()) => queue.complete(claim).await,
        Err(e) => queue.fail(claim, &e, false).await,
    }
}

impl Oracle {
    /// With faults off and the clock past every lease and backoff, run the
    /// derive queue dry; every job must finish and every trunk version be
    /// derived.
    pub async fn check_jobs(&mut self, world: &World, settle_at: i64, backoff_passed: i64) {
        rill_types::testing::set_clock(Some(settle_at));
        let queue = queue(&world.repo);
        let _ = queue.reap().await;
        if let Ok(dead) = queue.dead(DRAIN_LIMIT).await {
            for job in dead {
                let _ = queue.retry_dead(&job.job.id).await;
            }
        }
        // Jobs just taken back wait out a backoff from `settle_at`.
        rill_types::testing::set_clock(Some(backoff_passed));
        for _ in 0..DRAIN_LIMIT {
            match queue.claim(WORKER).await {
                Ok(Some(claim)) => {
                    let _ = run_job(world, &queue, &claim).await;
                }
                _ => break,
            }
        }
        let Ok(stats) = queue.stats().await else {
            return;
        };
        if stats.pending() != 0 || stats.dead != 0 || stats.leased != 0 {
            self.violate(
                Invariant::JobsFinish,
                format!("derive queue left {stats:?}"),
            );
        }
        for version in self.trunk().to_vec() {
            if !matches!(
                world
                    .derive
                    .has::<rill_core::derive::ChangeSummary>(&version)
                    .await,
                Ok(true)
            ) {
                self.violate(
                    Invariant::JobsFinish,
                    format!("trunk version {} was never derived", version.short()),
                );
            }
        }
    }
}
