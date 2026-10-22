//! The worker that follows trunk and verifies what landed.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rill_store::repo::LeaseHolder;
use rill_types::TrunkSeq;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::batch::BatchSize;
use super::record::{
    load_cursor, load_marks, recent_records, record_at, store_cursor, store_marks,
    VerificationRecord, VerificationStatus,
};
use crate::error::Result;
use crate::land::Lander;

/// Most consecutive landings verified as one batch by default.
pub const DEFAULT_BATCH_SIZE: u64 = 32;

/// How often the whole plan runs on the head by default; between, batches
/// run only the steps their landings touched.
pub const DEFAULT_FULL_EVERY: Duration = Duration::from_secs(15 * 60);

/// What one sweep did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sweep {
    /// Positions covered, `(from, to]`.
    pub from: TrunkSeq,
    pub to: TrunkSeq,
    /// Whether the batch tip passed.
    pub green: bool,
    /// Culprit found by bisection, if the batch went red on a green base.
    pub culprit: Option<TrunkSeq>,
    /// The tip ran the whole plan.
    pub full: bool,
}

/// Verifies trunk positions after they land, bisects red batches to the
/// first red position and, when policy allows, reverts it.
pub struct PostLandVerifier {
    pub(super) lander: Arc<Lander>,
    batch: BatchSize,
    full_every: Duration,
    last_full: parking_lot::Mutex<Option<Instant>>,
    /// One sweep at a time; a second caller waits rather than racing on
    /// the cursor.
    sweep_lock: Mutex<()>,
    lease: LeaseHolder,
}

/// The lease role the post-land verifier holds.
pub const POSTLAND_LEASE: &str = "postland";

impl PostLandVerifier {
    /// A worker over `lander`'s repository and verifier, verifying at most
    /// `max_batch` landings together.
    pub fn new(lander: Arc<Lander>, max_batch: u64) -> Arc<Self> {
        Self::with_full_every(lander, max_batch, DEFAULT_FULL_EVERY)
    }

    /// [`PostLandVerifier::new`], running the whole plan every
    /// `full_every`.
    pub fn with_full_every(lander: Arc<Lander>, max_batch: u64, full_every: Duration) -> Arc<Self> {
        Arc::new(PostLandVerifier {
            lease: LeaseHolder::new(lander.repo().clone(), POSTLAND_LEASE),
            lander,
            batch: BatchSize::new(max_batch),
            full_every,
            last_full: parking_lot::Mutex::new(None),
            sweep_lock: Mutex::new(()),
        })
    }

    /// The lander this worker reports to.
    pub fn lander(&self) -> &Arc<Lander> {
        &self.lander
    }

    /// Last trunk position verified through.
    pub async fn cursor(&self) -> Result<TrunkSeq> {
        load_cursor(self.lander.repo()).await
    }

    /// Recent records in sequence order.
    pub async fn recent(&self) -> Result<Vec<VerificationRecord>> {
        recent_records(self.lander.repo()).await
    }

    /// The record for a trunk position.
    pub async fn record_at(&self, seq: TrunkSeq) -> Result<Option<VerificationRecord>> {
        record_at(self.lander.repo(), seq).await
    }

    /// Verify the next batch after the cursor; `None` when trunk has not
    /// moved.
    pub async fn run_once(self: &Arc<Self>) -> Result<Option<Sweep>> {
        let _guard = self.sweep_lock.lock().await;
        // A standby verifier does nothing until it holds the lease.
        let Some(lease) = self.lease.current().await? else {
            return Ok(None);
        };
        let repo = self.lander.repo();
        let cursor = load_cursor(repo).await?;
        let Some((head, _)) = repo.trunk_head().await? else {
            return Ok(None);
        };
        if head <= cursor {
            return Ok(None);
        }
        let to = TrunkSeq(head.get().min(cursor.get() + self.batch.get()));
        let marks = load_marks(repo).await?;
        let mut next_marks = marks;
        let full = self.full_due();
        let base_culprit = match record_at(repo, cursor).await? {
            Some(record) if record.status == VerificationStatus::Red => {
                Some(record.culprit.unwrap_or(cursor))
            }
            _ => None,
        };

        // Only steps something touched since the last passing position
        // can have changed outcome.
        let tip = self.verify_seq(to, (!full).then_some(marks.green)).await?;
        if full {
            *self.last_full.lock() = Some(Instant::now());
        }
        let mut culprit = None;
        if tip.passed() {
            self.fill(
                cursor,
                to,
                VerificationStatus::Green,
                None,
                "passing batch tip",
            )
            .await?;
            next_marks.green = to;
            if full {
                next_marks.full_green = to;
            }
        } else if let Some(previous) = base_culprit {
            // Still red since an earlier culprit; nothing new to find.
            self.fill(
                cursor,
                to,
                VerificationStatus::Red,
                Some(previous),
                "still red",
            )
            .await?;
            self.set_culprit(to, previous).await?;
        } else {
            // A full run can find what selection missed before the cursor:
            // search from the last full pass.
            let from = if full { marks.full_green } else { cursor };
            let found = self.bisect_with(from, to, full).await?;
            culprit = Some(found);
            self.set_culprit(to, found).await?;
            self.fill(
                from,
                found,
                VerificationStatus::Green,
                None,
                "before culprit",
            )
            .await?;
            self.fill(
                found,
                to,
                VerificationStatus::Red,
                Some(found),
                "after culprit",
            )
            .await?;
            // Bisection verified some of these directly; they are red
            // because of the culprit too.
            for (seq, _) in repo.trunk_range(found, to).await? {
                if let Some(record) = record_at(repo, seq).await? {
                    if record.status == VerificationStatus::Red && record.culprit.is_none() {
                        self.set_culprit(seq, found).await?;
                    }
                }
            }
            // Everything before the culprit passed.
            next_marks.green = TrunkSeq(found.get() - 1);
            if full {
                next_marks.full_green = next_marks.green;
            }
        }
        self.batch.after(tip.passed());
        if let Err(e) = store_cursor(repo, cursor, to, &lease).await {
            self.lease.lost().await;
            return Err(e);
        }
        store_marks(repo, marks, next_marks, &lease).await?;
        Ok(Some(Sweep {
            from: cursor,
            to,
            green: tip.passed(),
            culprit,
            full,
        }))
    }

    /// Whether the next tip should run the whole plan.
    fn full_due(&self) -> bool {
        match *self.last_full.lock() {
            None => true,
            Some(at) => at.elapsed() >= self.full_every,
        }
    }

    /// Poll trunk every `interval` until `shutdown` is cancelled.
    pub async fn run_forever(self: Arc<Self>, interval: Duration, shutdown: CancellationToken) {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(interval) => {
                    match self.run_once().await {
                        Ok(Some(sweep)) => tracing::info!(?sweep, "post-land sweep"),
                        Ok(None) => {}
                        Err(e) => tracing::error!(error = %e, "post-land sweep failed"),
                    }
                }
            }
        }
        self.lease.release().await;
        tracing::info!("post-land verifier stopped");
    }
}
