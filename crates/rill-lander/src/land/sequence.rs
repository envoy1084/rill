//! The sequencer: one ordered pipeline of requests, each rebased onto the
//! head it will land on, approved on the review the landing commits
//! against, and landed in groups of up to [`MAX_BATCH`] per transaction.

use std::collections::HashSet;

use rill_core::{rebase, RootTree};
use rill_store::repo::{LandBatch, LandEntry, Lease};
use rill_types::{ChangeVersion, ChangeVersionHash, LandStatus, LandTicket, Timestamp, TrunkSeq};

use super::lander::{Lander, LANDER_LEASE};
use crate::config::VerifyMode;
use crate::error::{Error, Result};
use crate::events::LandEvent;
use crate::queue::QueuedRequest;
use crate::traits::Approval;

/// Landings committed together at most.
pub const MAX_BATCH: usize = 64;

/// A request ready to land: rebased, verified and approved.
struct Candidate {
    queued: QueuedRequest,
    version: ChangeVersionHash,
    cv: ChangeVersion,
    approval: Approval,
}

/// What became of preparing one request.
enum Prepared {
    Ready(Box<Candidate>),
    /// Finished without landing; its status is recorded.
    Dropped,
}

impl Lander {
    /// Serve `queued`, in order, in groups of up to [`MAX_BATCH`]. Returns
    /// how many requests were taken up.
    pub(super) async fn sequence(&self, queued: Vec<QueuedRequest>, lease: &Lease) -> usize {
        let count = queued.len();
        let mut marked = HashSet::new();
        for window in queued.chunks(MAX_BATCH) {
            if !self.land_window(window, lease, &mut marked).await {
                break;
            }
        }
        count
    }

    /// Land what can land of `window`; `false` once the lease is lost.
    async fn land_window(
        &self,
        window: &[QueuedRequest],
        lease: &Lease,
        marked: &mut HashSet<LandTicket>,
    ) -> bool {
        let mut live: Vec<QueuedRequest> = window.to_vec();
        for attempt in 0..=self.policy.max_retries {
            let head = match self.repo.trunk_head().await {
                Ok(head) => head,
                Err(e) => {
                    tracing::warn!(error = %e, "reading the trunk head failed");
                    return true;
                }
            };
            let mut chain: Vec<Candidate> = Vec::new();
            let mut tip = head.map(|(_, h)| h);
            for queued in &live {
                if marked.insert(queued.ticket) && !self.accept(queued).await {
                    continue;
                }
                match self.prepare(queued, tip).await {
                    Ok(Prepared::Ready(candidate)) => {
                        tip = Some(candidate.version);
                        chain.push(*candidate);
                    }
                    Ok(Prepared::Dropped) => {}
                    Err(e) => self.fail(queued, e.to_string()).await,
                }
            }
            live = chain.iter().map(|c| c.queued.clone()).collect();
            if chain.is_empty() {
                return true;
            }
            match self.commit(head, chain, lease).await {
                Ok(()) => return true,
                Err(Error::Fenced) => return false,
                Err(e) if is_conflict(&e) => {
                    tracing::debug!(attempt, "trunk or a review moved; sequencing again");
                }
                Err(e) => {
                    for queued in &live {
                        self.fail(queued, e.to_string()).await;
                    }
                    return true;
                }
            }
        }
        for queued in &live {
            self.fail(
                queued,
                format!(
                    "trunk moved {} times while landing; giving up",
                    self.policy.max_retries + 1
                ),
            )
            .await;
        }
        true
    }

    /// Take a request up: review must still let the lander have it.
    async fn accept(&self, queued: &QueuedRequest) -> bool {
        let change_id = queued.request.change_id;
        match self.hooks.mark_landing(change_id).await {
            Ok(()) => true,
            Err(e) => {
                let reason = format!("change is no longer landable: {e}");
                tracing::warn!(%change_id, ticket = %queued.ticket, reason, "landing refused");
                self.set_status(
                    queued,
                    LandStatus::Failed {
                        reason: reason.clone(),
                    },
                )
                .await;
                self.publish(LandEvent::Failed { change_id, reason });
                false
            }
        }
    }

    /// Rebase `queued` onto `tip` (the head it will land on), verify and
    /// approve it.
    async fn prepare(
        &self,
        queued: &QueuedRequest,
        tip: Option<ChangeVersionHash>,
    ) -> Result<Prepared> {
        let requested = queued.request.version;
        let cv = self
            .repo
            .get_change_version(&requested)
            .await?
            .ok_or(Error::ChangeNotFound(requested))?;
        if let Some(seq) = self.repo.trunk_seq_of(&requested).await? {
            self.fail(queued, format!("already landed at {seq}")).await;
            return Ok(Prepared::Dropped);
        }
        let (version, cv) = match tip {
            Some(tip) if cv.first_parent() != Some(tip) => {
                if self.graph.is_ancestor(&requested, &tip).await? {
                    self.fail(queued, "version is already an ancestor of trunk".into())
                        .await;
                    return Ok(Prepared::Dropped);
                }
                match self.rebase_onto(queued, &requested, &tip).await? {
                    Some(rebased) => rebased,
                    None => return Ok(Prepared::Dropped),
                }
            }
            _ => (requested, cv),
        };
        let mode = queued
            .request
            .verify_override
            .unwrap_or_else(|| self.policy.verify_mode(queued.request.risk));
        if mode != VerifyMode::None && !self.verify(queued, &version).await? {
            return Ok(Prepared::Dropped);
        }
        self.derive.ensure_land_critical(&version).await?;
        let approval = self
            .hooks
            .approve(
                queued.request.change_id,
                requested,
                &queued.request.requester,
            )
            .await?;
        if let Some(reason) = &approval.refusal {
            self.eject(queued, reason.clone()).await;
            return Ok(Prepared::Dropped);
        }
        self.set_status(queued, LandStatus::Landing).await;
        Ok(Prepared::Ready(Box::new(Candidate {
            queued: queued.clone(),
            version,
            cv,
            approval,
        })))
    }

    /// Land `chain` on `head` in one transaction, with every request's
    /// `Landed` status and follow-up jobs.
    async fn commit(
        &self,
        head: Option<(TrunkSeq, ChangeVersionHash)>,
        chain: Vec<Candidate>,
        lease: &Lease,
    ) -> Result<()> {
        let first = head.map(|(s, _)| s.get()).unwrap_or(0) + 1;
        let actor = crate::revert::lander_principal();
        let versions: Vec<ChangeVersionHash> = chain.iter().map(|c| c.version).collect();
        let mut also = super::followups::landing_jobs(&self.repo, &versions)?;
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
            expected: head.map(|(_, h)| h),
            entries,
            fence: Some(lease.clone()),
            actor: Some(actor),
            also,
        };
        let landings = match self.repo.land(batch).await {
            Ok(landings) => landings,
            Err(e) if e.is_conflict() => {
                if self.repo.lease_epoch(LANDER_LEASE).await? != lease.epoch {
                    self.lost_lease().await;
                    return Err(Error::Fenced);
                }
                return Err(e.into());
            }
            Err(e) => {
                // The land may have committed with its reply lost: trunk is
                // the record of what happened.
                match self.repo.landing_of(&chain[0].version).await {
                    Ok(Some(landing)) if landing.change_id == chain[0].cv.change_id => {
                        let mut landings = Vec::with_capacity(chain.len());
                        for c in &chain {
                            if let Some(l) = self.repo.landing_of(&c.version).await? {
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
        for (landing, candidate) in landings.into_iter().zip(&chain) {
            self.announce(&candidate.queued, landing, &candidate.cv)
                .await;
        }
        Ok(())
    }

    /// Take a request out of the queue because what allowed it no longer
    /// holds.
    async fn eject(&self, queued: &QueuedRequest, reason: String) {
        let change_id = queued.request.change_id;
        tracing::info!(%change_id, ticket = %queued.ticket, reason, "request ejected");
        self.set_status(
            queued,
            LandStatus::Ejected {
                reason: reason.clone(),
            },
        )
        .await;
        self.return_to_owner(change_id, reason.clone()).await;
        self.publish(LandEvent::Failed { change_id, reason });
    }

    /// Rebase `tip` onto `head`. Returns `None` after recording a
    /// conflict status.
    async fn rebase_onto(
        &self,
        queued: &QueuedRequest,
        tip: &ChangeVersionHash,
        head: &ChangeVersionHash,
    ) -> Result<Option<(ChangeVersionHash, ChangeVersion)>> {
        let rebased = rebase(&self.derive, tip, head, Timestamp::now()).await?;
        if rebased.has_conflicts() {
            let paths: Vec<_> = rebased.conflicted_paths().cloned().collect();
            let change_id = queued.request.change_id;
            tracing::info!(%change_id, ?paths, "rebase conflicted");
            self.set_status(
                queued,
                LandStatus::Conflict {
                    paths: paths.clone(),
                },
            )
            .await;
            self.return_to_owner(
                change_id,
                format!("rebase onto trunk conflicts at {} path(s)", paths.len()),
            )
            .await;
            self.publish(LandEvent::Conflict { change_id, paths });
            return Ok(None);
        }
        let hash = self.repo.put_change_version(&rebased).await?;
        Ok(Some((hash, rebased)))
    }

    /// Run pre-land verification. Returns `false` after recording a
    /// failure status.
    async fn verify(&self, queued: &QueuedRequest, hash: &ChangeVersionHash) -> Result<bool> {
        self.set_status(queued, LandStatus::Verifying).await;
        let root = self.derive.get::<RootTree>(hash).await?;
        let outcome =
            match tokio::time::timeout(self.policy.max_wait, self.verifier.verify(root, *hash))
                .await
            {
                Ok(outcome) => outcome?,
                Err(_) => {
                    self.fail(
                        queued,
                        format!("verification exceeded {:?}", self.policy.max_wait),
                    )
                    .await;
                    return Ok(false);
                }
            };
        if !outcome.green {
            self.fail(queued, format!("verification failed: {}", outcome.details))
                .await;
            return Ok(false);
        }
        Ok(true)
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
        match self.hooks.dependents_of(cv.change_id).await {
            Ok(dependents) if !dependents.is_empty() => {
                self.publish(LandEvent::DependentsReady {
                    landed: cv.change_id,
                    dependents,
                });
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!(change_id = %cv.change_id, error = %e, "dependents_of hook failed");
            }
        }
    }
}

fn is_conflict(e: &Error) -> bool {
    matches!(e, Error::Store(s) if s.is_conflict())
}
