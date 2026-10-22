//! A lander process that was replaced but still holds its old lease, as
//! a paused process does when it wakes up, trying to land.

use rill_lander::land::LANDER_LEASE;
use rill_store::repo::{LandBatch, LandEntry};
use rill_types::RiskClass;

use crate::actors::{Ctx, Outcome};
use crate::oracle::Invariant;

impl Ctx<'_> {
    /// Once another lander holds the lease, the old holder's landing must
    /// be refused, even when the store reports otherwise.
    pub(crate) async fn zombie_land(&mut self) -> Outcome {
        let Some(stale) = self.world.zombie.clone() else {
            return Outcome::Skipped;
        };
        let repo = &self.world.repo;
        match repo.lease_epoch(LANDER_LEASE).await {
            Ok(epoch) if epoch != stale.epoch => {}
            Ok(_) => return Outcome::Skipped,
            Err(_) => return Outcome::Failed,
        }
        let roll = self.rng.below(usize::MAX >> 1);
        let Some(version) = self.oracle.acknowledged_at(roll) else {
            return Outcome::Skipped;
        };
        let (Ok(head), Ok(Some(cv))) = (
            repo.trunk_head().await,
            repo.get_change_version(&version).await,
        ) else {
            return Outcome::Failed;
        };
        let batch = LandBatch {
            line: Default::default(),
            expected: head.map(|(_, h)| h),
            entries: vec![LandEntry {
                version,
                change_id: cv.change_id,
                requested_version: version,
                requester: cv.author,
                risk: RiskClass::Normal,
                review: None,
                policy: None,
                approvals: Vec::new(),
            }],
            fence: Some(stale),
            ..LandBatch::default()
        };
        self.oracle.fenced_attempts += 1;
        let refused = repo.land(batch).await.is_err();
        // A lost acknowledgement can hide a commit, so look at trunk.
        let moved = match repo.trunk_head().await {
            Ok(now) => now != head,
            Err(_) => !refused,
        };
        if moved {
            self.oracle.violate(
                Invariant::DeposedHolderFenced,
                format!("a replaced lander landed {}", version.short()),
            );
        }
        Outcome::Done
    }
}
