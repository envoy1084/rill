//! Speculation: requests that touch nothing in common are rebased onto
//! the trunk head in parallel; a request that overlaps earlier ones is
//! rebased on top of them. At commit the batch is lined up: each request
//! is re-parented onto the one before it, which for disjoint changes is a
//! pure tree operation that cannot conflict.

use futures::future::join_all;
use rill_core::rebase;
use rill_types::{ChangeVersionHash, Timestamp};

use super::lander::Lander;
use super::prepare::{Candidate, Prepared};
use crate::error::Result;
use crate::queue::QueuedRequest;
use crate::schedule::{Footprint, Plan, Slot};

impl Lander {
    /// Prepare `batch` (in commit order) as a chain on `head`.
    pub(super) async fn speculate(
        &self,
        head: Option<ChangeVersionHash>,
        batch: &[(QueuedRequest, Footprint)],
    ) -> Vec<Candidate> {
        let footprints: Vec<Footprint> = batch.iter().map(|(_, f)| f.clone()).collect();
        let plan = Plan::new(&footprints);
        let n = plan.len();
        let mut ready: Vec<Option<Candidate>> = (0..n).map(|_| None).collect();
        let mut alive = vec![true; n];
        let mut placed = vec![Slot::Head; n];
        for wave in plan.waves() {
            let mut jobs = Vec::new();
            for i in wave {
                let slot = plan.slot(i, &alive, &placed);
                placed[i] = slot.clone();
                let base = match slot {
                    Slot::Head => head,
                    Slot::After(j) => ready[j].as_ref().map(|c| c.version),
                    Slot::Linear => continue,
                };
                jobs.push(async move { (i, self.prepare(&batch[i].0, base).await) });
            }
            for (i, outcome) in join_all(jobs).await {
                let conflicted = matches!(outcome, Ok(Prepared::Conflicted));
                self.thrash.record(&batch[i].1.domains(), conflicted);
                match outcome {
                    Ok(Prepared::Ready(candidate)) => ready[i] = Some(*candidate),
                    Ok(_) => alive[i] = false,
                    Err(e) => {
                        alive[i] = false;
                        self.fail(&batch[i].0, e.to_string()).await;
                    }
                }
            }
        }
        let mut chain: Vec<Candidate> = Vec::new();
        let mut tip = head;
        for i in 0..n {
            if !alive[i] {
                continue;
            }
            let lined = match (placed[i] == Slot::Linear, ready[i].take()) {
                (true, _) => match self.prepare(&batch[i].0, tip).await {
                    Ok(Prepared::Ready(c)) => Some(*c),
                    Ok(_) => None,
                    Err(e) => {
                        self.fail(&batch[i].0, e.to_string()).await;
                        None
                    }
                },
                (false, Some(c)) => match self.reparent(c, tip).await {
                    Ok(c) => c,
                    Err(e) => {
                        self.fail(&batch[i].0, e.to_string()).await;
                        None
                    }
                },
                (false, None) => None,
            };
            if let Some(c) = lined {
                tip = Some(c.version);
                chain.push(c);
            }
        }
        chain
    }

    /// `candidate` on `tip`. It was speculated against an earlier state
    /// holding everything it overlaps, so this only adds disjoint changes.
    async fn reparent(
        &self,
        candidate: Candidate,
        tip: Option<ChangeVersionHash>,
    ) -> Result<Option<Candidate>> {
        let Some(tip) = tip else {
            return Ok(Some(candidate));
        };
        if candidate.base == Some(tip) || candidate.cv.first_parent() == Some(tip) {
            return Ok(Some(candidate));
        }
        let rebased = rebase(&self.derive, &candidate.version, &tip, Timestamp::now()).await?;
        if rebased.has_conflicts() {
            let paths: Vec<_> = rebased.conflicted_paths().cloned().collect();
            self.conflicted(&candidate.queued, paths).await;
            return Ok(None);
        }
        let version = self.repo.put_change_version(&rebased).await?;
        self.derive.ensure_land_critical(&version).await?;
        Ok(Some(Candidate {
            version,
            cv: rebased,
            base: Some(tip),
            ..candidate
        }))
    }
}
