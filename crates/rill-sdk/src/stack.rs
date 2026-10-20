//! Client-side stack helpers: submit, sync and land a chain of changes
//! bottom-up, composed from review and repo RPCs.

use std::time::{Duration, Instant};

use rill_types::{ChangeId, ChangeVersionHash, RepoPathBuf, ReviewState, TrunkSeq};

use crate::error::{Error, Result};
use crate::repo::RepoClient;
use crate::review::ReviewClient;
use crate::types::{LandProgress, LandStatus, Root};

/// Polling schedule for [`StackClient::await_land`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backoff {
    /// First wait between polls.
    pub initial: Duration,
    /// Growth factor applied after each poll.
    pub factor: f64,
    /// Longest wait between polls.
    pub max: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff {
            initial: Duration::from_millis(250),
            factor: 1.6,
            max: Duration::from_secs(5),
        }
    }
}

impl Backoff {
    fn next(&self, current: Duration) -> Duration {
        current.mul_f64(self.factor).min(self.max)
    }
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
            backoff: Backoff::default(),
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

    /// Land the stack bottom-up, waiting for each landing to finish before
    /// requesting the next. `timeout` bounds each individual landing.
    pub async fn land_stack(
        &self,
        change_ids: &[ChangeId],
        timeout: Duration,
    ) -> Result<Vec<LandedChange>> {
        let mut landed = Vec::with_capacity(change_ids.len());
        for id in change_ids {
            let ticket = self.reviews.request_land(*id).await?;
            let progress = self.await_land(&ticket.id, timeout).await?;
            let (seq, hash) = match (progress.seq, progress.landed_hash) {
                (Some(seq), Some(hash)) => (seq, hash),
                _ => {
                    return Err(Error::LandFailed {
                        ticket: ticket.id,
                        status: LandStatus::Landed,
                        message: "landed without a sequence or hash".into(),
                    })
                }
            };
            landed.push(LandedChange {
                change_id: *id,
                seq,
                hash,
            });
        }
        Ok(landed)
    }

    /// Poll a land ticket with backoff until it lands. Fails with
    /// [`Error::LandFailed`] on a failed or cancelled landing and
    /// [`Error::Timeout`] when `timeout` elapses first.
    pub async fn await_land(&self, ticket: &str, timeout: Duration) -> Result<LandProgress> {
        let started = Instant::now();
        let mut wait = self.backoff.initial;
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
            tokio::time::sleep(wait.min(timeout - elapsed)).await;
            wait = self.backoff.next(wait);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        let b = Backoff {
            initial: Duration::from_millis(100),
            factor: 2.0,
            max: Duration::from_millis(350),
        };
        let w1 = b.next(b.initial);
        let w2 = b.next(w1);
        assert_eq!(w1, Duration::from_millis(200));
        assert_eq!(w2, Duration::from_millis(350));
    }
}
