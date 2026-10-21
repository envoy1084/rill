//! One request's journey: rebase, verify, compare-and-swap onto trunk.

use rill_core::{rebase, RootTree};
use rill_types::{ChangeVersion, ChangeVersionHash, Timestamp, TrunkSeq};

use rill_store::repo::{LandBatch, LandEntry};

use super::lander::{Lander, LANDER_LEASE};
use super::record::Landing;
use crate::config::VerifyMode;
use crate::error::{Error, Result};
use crate::events::LandEvent;
use crate::queue::{LandStatus, QueuedRequest};

/// Why an attempt did not reach trunk.
enum Attempt {
    /// Landed at this sequence.
    Landed(TrunkSeq),
    /// Trunk moved under us; `tip` is the version to rebase next time.
    Retry { tip: ChangeVersionHash },
    /// Terminal: the status has been recorded already.
    Done,
}

impl Lander {
    /// Drive `queued` to a terminal status. Never returns an error: every
    /// failure becomes a status so the caller's sweep continues.
    pub(super) async fn process(&self, queued: &QueuedRequest) {
        let change_id = queued.request.change_id;
        // Review refuses when the change is no longer landable: abandoned,
        // returned to its owner, or already landing through another request.
        // Landing anyway would bypass review or land the change twice.
        if let Err(e) = self.hooks.mark_landing(change_id).await {
            let reason = format!("change is no longer landable: {e}");
            tracing::warn!(%change_id, ticket = %queued.ticket, reason, "landing refused");
            self.set_status(
                queued.ticket,
                LandStatus::Failed {
                    reason: reason.clone(),
                },
            );
            self.publish(LandEvent::Failed { change_id, reason });
            return;
        }
        let mode = queued
            .request
            .verify_override
            .unwrap_or_else(|| self.policy.verify_mode(queued.request.risk));

        let mut tip = queued.request.version;
        for attempt in 0..=self.policy.max_retries {
            match self.attempt(queued, tip, mode).await {
                Ok(Attempt::Landed(seq)) => {
                    tracing::info!(%change_id, %seq, attempt, "landed");
                    return;
                }
                Ok(Attempt::Done) => return,
                Ok(Attempt::Retry { tip: next }) => {
                    tracing::debug!(%change_id, attempt, "trunk moved; retrying");
                    tip = next;
                }
                Err(e) => {
                    self.fail(queued, e.to_string()).await;
                    return;
                }
            }
        }
        self.fail(
            queued,
            format!(
                "trunk moved {} times while landing; giving up",
                self.policy.max_retries + 1
            ),
        )
        .await;
    }

    async fn attempt(
        &self,
        queued: &QueuedRequest,
        tip: ChangeVersionHash,
        mode: VerifyMode,
    ) -> Result<Attempt> {
        let head = self.repo.trunk_head().await?;
        let cv = self
            .repo
            .get_change_version(&tip)
            .await?
            .ok_or(Error::ChangeNotFound(tip))?;

        if let Some(seq) = self.repo.trunk_seq_of(&tip).await? {
            self.fail(queued, format!("already landed at {seq}")).await;
            return Ok(Attempt::Done);
        }

        let (hash, cv) = match head {
            Some((_, head_hash)) if cv.first_parent() != Some(head_hash) => {
                if self.graph.is_ancestor(&tip, &head_hash).await? {
                    self.fail(queued, "version is already an ancestor of trunk".into())
                        .await;
                    return Ok(Attempt::Done);
                }
                match self.rebase_onto(queued, &tip, &head_hash).await? {
                    Some(rebased) => rebased,
                    None => return Ok(Attempt::Done),
                }
            }
            _ => (tip, cv),
        };

        if mode != VerifyMode::None && !self.verify(queued, &hash).await? {
            return Ok(Attempt::Done);
        }

        self.set_status(queued.ticket, LandStatus::Landing);
        self.derive.ensure_land_critical(&hash).await?;
        let epoch = self.epoch().await?;
        let request = &queued.request;
        let batch = LandBatch {
            expected: head.map(|(_, h)| h),
            entries: vec![LandEntry {
                version: hash,
                change_id: cv.change_id,
                requested_version: request.version,
                requester: request.requester.clone(),
                risk: request.risk,
                review: self.repo.read_review(&cv.change_id).await?,
            }],
            fence: Some((LANDER_LEASE.to_string(), epoch)),
            actor: Some(crate::revert::lander_principal()),
        };
        match self.repo.land(batch).await {
            Ok(landings) => {
                let landing = landings.into_iter().next().ok_or_else(|| {
                    Error::Store(rill_store::Error::Backend("land returned no record".into()))
                })?;
                let seq = landing.seq;
                self.announce(queued, landing, &cv).await;
                Ok(Attempt::Landed(seq))
            }
            Err(e) if e.is_conflict() => {
                if self.repo.lease_epoch(LANDER_LEASE).await? != epoch {
                    return Err(Error::Fenced);
                }
                Ok(Attempt::Retry { tip: hash })
            }
            Err(e) => {
                // The land may have committed with its reply lost: trunk is
                // the record of what happened.
                if let Ok(Some(landing)) = self.repo.landing_of(&hash).await {
                    if landing.change_id == cv.change_id {
                        let seq = landing.seq;
                        self.announce(queued, landing, &cv).await;
                        return Ok(Attempt::Landed(seq));
                    }
                }
                Err(e.into())
            }
        }
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
                queued.ticket,
                LandStatus::Conflict {
                    paths: paths.clone(),
                },
            );
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
        self.set_status(queued.ticket, LandStatus::Verifying);
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
    async fn announce(&self, queued: &QueuedRequest, landing: Landing, cv: &ChangeVersion) {
        let seq = landing.seq;
        let version = landing.version;
        self.set_status(queued.ticket, LandStatus::Landed { seq });
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
