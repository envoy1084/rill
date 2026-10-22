//! Getting one request ready to land: rebase onto the tip, verify and
//! approve, or record why it cannot land.

use rill_core::{rebase, RootTree};
use rill_types::{ChangeVersion, ChangeVersionHash, LandStatus, RepoPathBuf, Timestamp};

use super::lander::Lander;
use crate::config::VerifyMode;
use crate::error::{Error, Result};
use crate::events::LandEvent;
use crate::queue::QueuedRequest;
use crate::traits::{Approval, VerifyRequest, VerifyStage};

/// A request ready to land: rebased, verified and approved.
pub(super) struct Candidate {
    pub(super) queued: QueuedRequest,
    pub(super) version: ChangeVersionHash,
    pub(super) cv: ChangeVersion,
    pub(super) approval: Approval,
    /// The state it was rebased onto.
    pub(super) base: Option<ChangeVersionHash>,
}

/// What became of preparing one request.
pub(super) enum Prepared {
    Ready(Box<Candidate>),
    /// Its rebase conflicted; its status is recorded.
    Conflicted,
    /// Finished without landing for another reason; its status is recorded.
    Dropped,
}

impl Lander {
    /// Rebase `queued` onto `tip` (the head it will land on), verify and
    /// approve it.
    pub(super) async fn prepare(
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
                    None => return Ok(Prepared::Conflicted),
                }
            }
            _ => (requested, cv),
        };
        let mode = queued
            .request
            .verify_override
            .unwrap_or_else(|| self.policy.verify_mode(queued.request.risk));
        // `Full` verifies everything; `Affected` only what changed since trunk.
        let base = tip.filter(|_| mode == VerifyMode::Affected);
        if mode != VerifyMode::None && !self.verify(queued, &version, base.as_ref()).await? {
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
            base: tip,
        })))
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
            self.conflicted(queued, paths).await;
            return Ok(None);
        }
        let hash = self.repo.put_change_version(&rebased).await?;
        Ok(Some((hash, rebased)))
    }

    /// Record that `queued` conflicts at `paths` and hand it back.
    pub(super) async fn conflicted(&self, queued: &QueuedRequest, paths: Vec<RepoPathBuf>) {
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
    }

    /// Run pre-land verification. Returns `false` after recording a
    /// failure status.
    async fn verify(
        &self,
        queued: &QueuedRequest,
        hash: &ChangeVersionHash,
        base: Option<&ChangeVersionHash>,
    ) -> Result<bool> {
        self.set_status(queued, LandStatus::Verifying).await;
        let root = self.derive.get::<RootTree>(hash).await?;
        let base = match base {
            Some(base) => Some(self.derive.get::<RootTree>(base).await?),
            None => None,
        };
        let request = VerifyRequest {
            root,
            version: *hash,
            base,
            risk: Some(queued.request.risk),
            stage: VerifyStage::PreLand,
        };
        let outcome =
            match tokio::time::timeout(self.policy.max_wait, self.verifier.verify(request)).await {
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
}
