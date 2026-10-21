//! The worker that follows trunk and verifies what landed.

use std::sync::Arc;
use std::time::Duration;

use rill_core::RootTree;
use rill_store::repo::LeaseHolder;
use rill_types::{Timestamp, TrunkSeq};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::record::{
    load_cursor, put_record, recent_records, record_at, store_cursor, VerificationRecord,
    VerificationStatus,
};
use crate::error::{Error, Result};
use crate::events::LandEvent;
use crate::land::Lander;
use crate::revert::schedule_auto_revert;

/// Default number of consecutive landings verified as one batch when the
/// worker is behind.
pub const DEFAULT_BATCH_SIZE: u64 = 8;

/// What one sweep did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sweep {
    /// Positions covered, `(from, to]`.
    pub from: TrunkSeq,
    pub to: TrunkSeq,
    /// Whether the batch tip verified green.
    pub green: bool,
    /// Culprit found by bisection, if the batch went red on a green base.
    pub culprit: Option<TrunkSeq>,
}

/// Verifies trunk positions after they land, bisects red batches to the
/// first red position and, when policy allows, reverts it.
pub struct PostLandVerifier {
    lander: Arc<Lander>,
    batch_size: u64,
    /// One sweep at a time; a second caller waits rather than racing on
    /// the cursor.
    sweep_lock: Mutex<()>,
    lease: LeaseHolder,
}

/// The lease role the post-land verifier holds.
pub const POSTLAND_LEASE: &str = "postland";

impl PostLandVerifier {
    /// A worker over `lander`'s repository and verifier.
    pub fn new(lander: Arc<Lander>, batch_size: u64) -> Arc<Self> {
        Arc::new(PostLandVerifier {
            lease: LeaseHolder::new(lander.repo().clone(), POSTLAND_LEASE),
            lander,
            batch_size: batch_size.max(1),
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
        let to = TrunkSeq(head.get().min(cursor.get() + self.batch_size));
        let base_culprit = if cursor == TrunkSeq::ZERO {
            None
        } else {
            match record_at(repo, cursor).await? {
                Some(record) if record.status == VerificationStatus::Red => {
                    Some(record.culprit.unwrap_or(cursor))
                }
                _ => None,
            }
        };

        let tip = self.verify_seq(to).await?;
        let mut culprit = None;
        if tip.is_green() {
            self.fill(
                cursor,
                to,
                VerificationStatus::Green,
                None,
                "green batch tip",
            )
            .await?;
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
            let found = self.bisect(cursor, to).await?;
            culprit = Some(found);
            self.set_culprit(to, found).await?;
            self.fill(
                cursor,
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
        }
        if let Err(e) = store_cursor(repo, to, &lease).await {
            self.lease.lost().await;
            return Err(e);
        }
        Ok(Some(Sweep {
            from: cursor,
            to,
            green: tip.is_green(),
            culprit,
        }))
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
        tracing::info!("post-land verifier stopped");
    }

    /// Verify one position directly and record the result. A result that
    /// disagrees with an earlier direct verification marks the position
    /// flaky.
    pub async fn verify_seq(&self, seq: TrunkSeq) -> Result<VerificationRecord> {
        let repo = self.lander.repo();
        let version = repo.trunk_at(seq).await?.ok_or(Error::NotLanded(seq))?;
        let root = self.lander.derive().get::<RootTree>(&version).await?;
        let outcome = self.lander.verifier().verify(root, version).await?;
        let mut status = if outcome.green {
            VerificationStatus::Green
        } else {
            VerificationStatus::Red
        };
        if let Some(previous) = record_at(repo, seq).await? {
            if !previous.inferred && previous.status != status {
                tracing::warn!(%seq, ?previous.status, ?status, "verification result changed");
                status = VerificationStatus::Flaky;
            }
        }
        let record = VerificationRecord {
            seq,
            version,
            status,
            details: outcome.details,
            culprit: None,
            at: Timestamp::now(),
            inferred: false,
        };
        put_record(repo, &record).await?;
        self.lander.publish(LandEvent::Verification {
            seq,
            green: outcome.green,
        });
        Ok(record)
    }

    /// Find the first red position in `(green, red]`, where `green` is a
    /// known-green position (or zero) and `red` a known-red one.
    ///
    /// Verifier results are content-addressed in practice, so re-verifying
    /// a position costs nothing beyond the first time.
    pub async fn bisect(self: &Arc<Self>, green: TrunkSeq, red: TrunkSeq) -> Result<TrunkSeq> {
        let mut lo = green.get();
        let mut hi = red.get();
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            let record = self.verify_seq(TrunkSeq(mid)).await?;
            if record.is_green() {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let culprit = TrunkSeq(hi);
        self.set_culprit(culprit, culprit).await?;
        tracing::warn!(%culprit, "post-land verification found a culprit");
        self.lander.publish(LandEvent::Culprit { seq: culprit });
        if self.lander.policy().auto_revert {
            schedule_auto_revert(self.lander.clone(), culprit);
        }
        Ok(culprit)
    }

    /// Record `status` for every position in `(from, to)` that has no
    /// direct record yet.
    async fn fill(
        &self,
        from: TrunkSeq,
        to: TrunkSeq,
        status: VerificationStatus,
        culprit: Option<TrunkSeq>,
        why: &str,
    ) -> Result<()> {
        let repo = self.lander.repo();
        for (seq, version) in repo.trunk_range(from, to).await? {
            if seq == to {
                break;
            }
            if record_at(repo, seq).await?.is_some() {
                continue;
            }
            let record = VerificationRecord {
                seq,
                version,
                status,
                details: format!("inferred: {why} ({from}..{to})"),
                culprit,
                at: Timestamp::now(),
                inferred: true,
            };
            put_record(repo, &record).await?;
            self.lander.publish(LandEvent::Verification {
                seq,
                green: status == VerificationStatus::Green,
            });
        }
        Ok(())
    }

    async fn set_culprit(&self, seq: TrunkSeq, culprit: TrunkSeq) -> Result<()> {
        let repo = self.lander.repo();
        if let Some(mut record) = record_at(repo, seq).await? {
            record.culprit = Some(culprit);
            put_record(repo, &record).await?;
        }
        Ok(())
    }
}
