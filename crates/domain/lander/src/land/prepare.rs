//! Getting one request ready to land: rebase onto the tip, verify and
//! approve, or record why it cannot land.

use rill_core::{rebase, RootTree};
use rill_types::{ChangeVersion, ChangeVersionHash, LandStatus, Timestamp};

use super::head_config::HeadRules;
use super::lander::Lander;
use crate::config::VerifyMode;
use crate::error::{Error, Result};
use crate::events::LandEvent;
use crate::ladder::{Climb, Unresolved};
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
        rules: &HeadRules,
    ) -> Result<Prepared> {
        let requested = queued.request.version;
        let cv = self
            .repo
            .get_change_version(&requested)
            .await?
            .ok_or(Error::ChangeNotFound(requested))?;
        if let Some(seq) = self.repo.line_seq_of(&self.line, &requested).await? {
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
                match self
                    .rebase_through_ladder(queued, &requested, &tip, rules)
                    .await?
                {
                    Some(rebased) => {
                        let hash = self.derive.store_version(&rebased).await?;
                        (hash, rebased)
                    }
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
        // A request cancelled while it was verified is no longer queued;
        // claiming it for landing fails then, and it is dropped.
        if !self.set_status(queued, LandStatus::Landing).await {
            return Ok(Prepared::Dropped);
        }
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

    /// Rebase `version` onto `head`, taking what conflicts up the
    /// ladder. Returns `None` after recording a conflict status.
    pub(super) async fn rebase_through_ladder(
        &self,
        queued: &QueuedRequest,
        version: &ChangeVersionHash,
        head: &ChangeVersionHash,
        rules: &HeadRules,
    ) -> Result<Option<ChangeVersion>> {
        let rebased = rebase(&self.derive, version, head, Timestamp::now()).await?;
        match self
            .climb(rebased, &rules.merge, queued.request.risk)
            .await?
        {
            Climb::Resolved(cv) => Ok(Some(*cv)),
            Climb::Unresolved(unresolved) => {
                self.conflicted(queued, unresolved, *head).await;
                Ok(None)
            }
        }
    }

    /// Record that `queued` conflicts and hand it back: to a resolver
    /// agent when one takes it, and always to its owner.
    pub(super) async fn conflicted(
        &self,
        queued: &QueuedRequest,
        unresolved: Unresolved,
        onto: ChangeVersionHash,
    ) {
        let change_id = queued.request.change_id;
        let paths = unresolved.paths;
        tracing::info!(%change_id, ?paths, reason = ?unresolved.reason, "rebase conflicted");
        self.set_status(
            queued,
            LandStatus::Conflict {
                paths: paths.clone(),
            },
        )
        .await;
        let mut reason = format!(
            "rebase onto {} conflicts at {} path(s)",
            self.line,
            paths.len()
        );
        if let Some(why) = &unresolved.reason {
            reason.push_str(&format!(": {why}"));
        }
        if let (false, Some(resolver)) = (unresolved.manual, &self.resolver) {
            match resolver
                .dispatch(change_id, queued.request.version, onto, &paths)
                .await
            {
                Ok(Some(who)) => reason.push_str(&format!("; {who} is resolving it")),
                Ok(None) => {}
                Err(e) => tracing::warn!(%change_id, error = %e, "conflict resolver failed"),
            }
        }
        self.return_to_owner(change_id, reason).await;
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
