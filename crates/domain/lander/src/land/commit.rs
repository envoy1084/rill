//! The land transaction for a prepared chain and what is reported after.

use rill_store::repo::{LandBatch, LandEntry, Lease};
use std::collections::HashSet;

use rill_types::{ChangeId, ChangeVersion, ChangeVersionHash, LandStatus, Timestamp, TrunkSeq};

use super::lander::{lander_lease, Lander};
use super::prepare::Candidate;
use crate::error::{Error, Result};
use crate::events::LandEvent;
use crate::queue::QueuedRequest;

impl Lander {
    /// Land `chain` on `head` in one transaction, with every request's
    /// `Landed` status and follow-up jobs.
    pub(super) async fn commit(
        &self,
        head: Option<(TrunkSeq, ChangeVersionHash)>,
        chain: Vec<Candidate>,
        lease: &Lease,
    ) -> Result<()> {
        let first = head.map(|(s, _)| s.get()).unwrap_or(0) + 1;
        let actor = crate::revert::lander_principal();
        let versions: Vec<ChangeVersionHash> = chain.iter().map(|c| c.version).collect();
        let mut also = super::followups::landing_jobs(&self.repo, &versions, &self.line)?;
        for (i, candidate) in chain.iter().enumerate() {
            let seq = TrunkSeq(first + i as u64);
            also = also.merge(self.repo.land_status_tx(
                &candidate.queued.record(),
                LandStatus::Landed { seq },
                &actor,
            )?);
        }
        let entries = chain
            .iter()
            .map(|c| LandEntry {
                version: c.version,
                change_id: c.cv.change_id,
                requested_version: c.queued.request.version,
                requester: c.queued.request.requester.clone(),
                risk: c.queued.request.risk,
                review: c.approval.review.clone(),
                policy: c.approval.policy,
                approvals: c.approval.votes.clone(),
            })
            .collect();
        let batch = LandBatch {
            line: self.line.clone(),
            expected: head.map(|(_, h)| h),
            entries,
            fence: Some(lease.clone()),
            actor: Some(actor),
            also,
        };
        let landings = match self.repo.land(batch).await {
            Ok(landings) => landings,
            Err(e) if e.is_conflict() => {
                if self.repo.lease_epoch(&lander_lease(&self.line)).await? != lease.epoch {
                    self.lost_lease().await;
                    return Err(Error::Fenced);
                }
                return Err(e.into());
            }
            Err(e) => {
                // The land may have committed with its reply lost: trunk is
                // the record of what happened.
                match self.repo.landing_of_on(&self.line, &chain[0].version).await {
                    Ok(Some(landing)) if landing.change_id == chain[0].cv.change_id => {
                        let mut landings = Vec::with_capacity(chain.len());
                        for c in &chain {
                            if let Some(l) = self.repo.landing_of_on(&self.line, &c.version).await?
                            {
                                landings.push(l);
                            }
                        }
                        landings
                    }
                    _ => return Err(e.into()),
                }
            }
        };
        tracing::info!(landed = landings.len(), first, "batch landed");
        let now = Timestamp::now().millis();
        for candidate in &chain {
            self.timings
                .landed(candidate.queued.request.requested_at.millis(), now);
        }
        let landed: Vec<_> = landings.iter().map(|l| (l.change_id, l.version)).collect();
        for (landing, candidate) in landings.into_iter().zip(&chain) {
            self.announce(&candidate.queued, landing, &candidate.cv)
                .await;
        }
        self.restack(&landed).await;
        Ok(())
    }

    /// Rebase what is stacked on the changes in `landed` onto them: for
    /// each, unless the change directly above it landed in the same batch.
    async fn restack(&self, landed: &[(ChangeId, ChangeVersionHash)]) {
        let ids: HashSet<ChangeId> = landed.iter().map(|(c, _)| *c).collect();
        for (change, version) in landed {
            let dependents = match self.hooks.dependents_of(*change).await {
                Ok(dependents) => dependents,
                Err(e) => {
                    tracing::error!(%change, error = %e, "dependents_of hook failed");
                    continue;
                }
            };
            if dependents.first().is_none_or(|above| ids.contains(above)) {
                continue;
            }
            match self.hooks.restack(*change, *version).await {
                Ok(restacked) if !restacked.is_empty() => {
                    self.publish(LandEvent::DependentsRestacked {
                        landed: *change,
                        restacked,
                    });
                }
                Ok(_) => {}
                Err(e) => tracing::error!(%change, error = %e, "restack hook failed"),
            }
        }
    }

    /// Report a landing the land transaction recorded.
    async fn announce(
        &self,
        queued: &QueuedRequest,
        landing: rill_types::Landing,
        cv: &ChangeVersion,
    ) {
        let seq = landing.seq;
        let version = landing.version;
        tracing::debug!(ticket = %queued.ticket, %seq, "request landed");
        if let Err(e) = self.hooks.mark_landed(cv.change_id, seq, version).await {
            tracing::error!(change_id = %cv.change_id, error = %e, "mark_landed hook failed");
        }
        self.publish(LandEvent::Landed {
            seq,
            change_id: cv.change_id,
            version,
        });
    }
}
