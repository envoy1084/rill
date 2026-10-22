//! Client-side stack helpers: submit, sync and land a chain of changes
//! bottom-up, composed from review and repo RPCs.

use std::time::{Duration, Instant};

use rill_types::{ChangeId, ChangeVersionHash, RepoPathBuf, ReviewState, TrunkSeq};

use crate::error::{Error, Result};
use crate::repo::RepoClient;
use crate::review::ReviewClient;
use crate::types::{LandProgress, LandStatus, Root};

/// The polling schedule for awaiting landings: exponential, without a
/// limit on attempts. Not jittered: one client polls its own ticket, and
/// jitter would only delay noticing the landing.
pub type Backoff = backon::ExponentialBuilder;

fn default_backoff() -> Backoff {
    backon::ExponentialBuilder::default()
        .with_min_delay(Duration::from_millis(250))
        .with_factor(1.6)
        .with_max_delay(Duration::from_secs(5))
        .without_max_times()
}

/// One change's new version after a sync, with any conflicted paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedChange {
    /// The change.
    pub change_id: ChangeId,
    /// Version before the rebase.
    pub from: ChangeVersionHash,
    /// Version after the rebase.
    pub to: ChangeVersionHash,
    /// Paths left in conflict; empty when the rebase was clean.
    pub conflicts: Vec<RepoPathBuf>,
}

/// Result of [`StackClient::sync_stack`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncResult {
    /// One entry per change, bottom to top.
    pub changes: Vec<SyncedChange>,
}

impl SyncResult {
    /// True when no change conflicts.
    pub fn is_clean(&self) -> bool {
        self.changes.iter().all(|c| c.conflicts.is_empty())
    }

    /// The new hashes, bottom to top.
    pub fn hashes(&self) -> Vec<ChangeVersionHash> {
        self.changes.iter().map(|c| c.to).collect()
    }
}

/// One change landed by [`StackClient::land_stack`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandedChange {
    /// The change.
    pub change_id: ChangeId,
    /// Its trunk sequence.
    pub seq: TrunkSeq,
    /// The landed version.
    pub hash: ChangeVersionHash,
}

/// A change of a stack that did not land, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlandedChange {
    /// The change.
    pub change_id: ChangeId,
    /// Why it did not land.
    pub message: String,
}

/// What [`StackClient::land_stack`] landed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StackLanding {
    /// Bottom first.
    pub landed: Vec<LandedChange>,
    /// The changes that did not land, bottom first: every change for a
    /// stack that lands whole, the ones from the first failure for a
    /// prefix.
    pub unlanded: Vec<UnlandedChange>,
}

/// Helpers that treat a list of change ids as a stack, bottom first.
#[derive(Clone, Debug)]
pub struct StackClient {
    repo: RepoClient,
    reviews: ReviewClient,
    backoff: Backoff,
}

impl StackClient {
    pub(crate) fn new(repo: RepoClient) -> Self {
        let reviews = repo.reviews();
        StackClient {
            repo,
            reviews,
            backoff: default_backoff(),
        }
    }

    /// Use a custom polling schedule when awaiting landings.
    pub fn with_backoff(mut self, backoff: Backoff) -> Self {
        self.backoff = backoff;
        self
    }

    /// Submit every change for review, then declare them a stack.
    /// Returns the review states after the stack is set.
    pub async fn submit_stack(&self, change_ids: &[ChangeId]) -> Result<Vec<ReviewState>> {
        for id in change_ids {
            self.reviews.submit(*id).await?;
        }
        self.reviews.set_stack(change_ids).await
    }

    /// Rebase the stack onto `onto`, bottom-up: each change's current
    /// version is rebased onto the previous result. Conflicts are reported,
    /// not raised, so the whole stack is always rebased.
    pub async fn sync_stack(&self, change_ids: &[ChangeId], onto: &Root) -> Result<SyncResult> {
        let mut parent = self.repo.resolve_root(onto).await?;
        let mut result = SyncResult::default();
        for id in change_ids {
            let current = self.repo.current_version(*id).await?.hash;
            let rebased = self.repo.rebase(current, parent).await?;
            tracing::debug!(
                change = %id,
                from = %current,
                to = %rebased.hash,
                conflicts = rebased.version.has_conflicts(),
                "rebased stack entry"
            );
            parent = rebased.hash;
            result.changes.push(SyncedChange {
                change_id: *id,
                from: current,
                to: rebased.hash,
                conflicts: rebased.conflicted_paths(),
            });
        }
        Ok(result)
    }

    /// Declare `change_ids` (bottom first) a stack and land it on `onto`
    /// (trunk when `None`) as one ordered group: every change in one
    /// landing or none, or with `prefix` the longest run from the bottom
    /// that can land. Waits up to `timeout` for each landing to finish.
    pub async fn land_stack(
        &self,
        change_ids: &[ChangeId],
        prefix: bool,
        onto: Option<&str>,
        timeout: Duration,
    ) -> Result<StackLanding> {
        let Some(top) = change_ids.last() else {
            return Ok(StackLanding::default());
        };
        if change_ids.len() > 1 {
            self.reviews.set_stack(change_ids).await?;
        }
        let tickets = self.reviews.request_land_stack(*top, prefix, onto).await?;
        let mut out = StackLanding::default();
        for ticket in tickets {
            match self.await_land(&ticket.id, timeout).await {
                Ok(progress) => match (progress.seq, progress.landed_hash) {
                    (Some(seq), Some(hash)) => out.landed.push(LandedChange {
                        change_id: ticket.change_id,
                        seq,
                        hash,
                    }),
                    _ => out.unlanded.push(UnlandedChange {
                        change_id: ticket.change_id,
                        message: "landed without a sequence or hash".into(),
                    }),
                },
                Err(Error::LandFailed { message, .. }) => out.unlanded.push(UnlandedChange {
                    change_id: ticket.change_id,
                    message,
                }),
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    /// Poll a land ticket with backoff until it lands. Fails with
    /// [`Error::LandFailed`] on a failed or cancelled landing and
    /// [`Error::Timeout`] when `timeout` elapses first.
    pub async fn await_land(&self, ticket: &str, timeout: Duration) -> Result<LandProgress> {
        let started = Instant::now();
        let mut waits = backon::BackoffBuilder::build(self.backoff);
        loop {
            let progress = self.reviews.land_status(ticket).await?;
            match progress.status() {
                LandStatus::Landed => return Ok(progress),
                status @ (LandStatus::Failed | LandStatus::Cancelled) => {
                    return Err(Error::LandFailed {
                        ticket: ticket.to_string(),
                        status,
                        message: progress.error.unwrap_or_default(),
                    })
                }
                LandStatus::Queued | LandStatus::Rebasing => {}
            }
            let elapsed = started.elapsed();
            if elapsed >= timeout {
                return Err(Error::Timeout(timeout));
            }
            let wait = waits.next().unwrap_or(Duration::from_secs(5));
            tokio::time::sleep(wait.min(timeout - elapsed)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use backon::BackoffBuilder;

    use super::*;

    #[test]
    fn land_polling_checks_at_its_first_interval_without_jitter() {
        let delays: Vec<u128> = default_backoff()
            .build()
            .take(3)
            .map(|d| d.as_millis())
            .collect();
        assert_eq!(
            delays,
            [250, 400, 640],
            "a landing is noticed at the first poll, not up to twice as late"
        );
    }
}
