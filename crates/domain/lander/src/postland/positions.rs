//! Verifying and recording single trunk positions.

use rill_core::RootTree;
use rill_types::{Timestamp, TrunkSeq};

use super::record::{put_record, record_at, VerificationRecord, VerificationStatus};
use super::worker::PostLandVerifier;
use crate::error::{Error, Result};
use crate::events::LandEvent;
use crate::traits::{VerifyRequest, VerifyStage};

impl PostLandVerifier {
    /// Verify one position directly and record the result. With `since`,
    /// only the steps something since that position touched run (it must
    /// have passed); without, the whole plan. A result that disagrees with
    /// an earlier direct verification marks the position flaky.
    pub async fn verify_seq(
        &self,
        seq: TrunkSeq,
        since: Option<TrunkSeq>,
    ) -> Result<VerificationRecord> {
        let repo = self.lander.repo();
        let derive = self.lander.derive();
        let version = repo.trunk_at(seq).await?.ok_or(Error::NotLanded(seq))?;
        let root = derive.get::<RootTree>(&version).await?;
        let base = match since.filter(|s| *s != TrunkSeq::ZERO && *s < seq) {
            Some(since) => match repo.trunk_at(since).await? {
                Some(before) => Some(derive.get::<RootTree>(&before).await?),
                None => None,
            },
            None => None,
        };
        let outcome = self
            .lander
            .verifier()
            .verify(VerifyRequest {
                root,
                version,
                base,
                risk: None,
                stage: VerifyStage::PostLand,
            })
            .await?;
        let mut status = match (outcome.green, outcome.flaky) {
            (true, false) => VerificationStatus::Green,
            (true, true) => VerificationStatus::Flaky,
            (false, _) => VerificationStatus::Red,
        };
        if let Some(previous) = record_at(repo, seq).await? {
            if !previous.inferred && previous.passed() != (status != VerificationStatus::Red) {
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
            failures: outcome.failures,
        };
        put_record(repo, &record).await?;
        self.lander.publish(LandEvent::Verification {
            seq,
            green: record.passed(),
        });
        Ok(record)
    }

    /// Record `status` for every position in `(from, to)` that was not
    /// verified directly.
    pub(super) async fn fill(
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
            if record_at(repo, seq).await?.is_some_and(|r| !r.inferred) {
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
                failures: Vec::new(),
            };
            put_record(repo, &record).await?;
            self.lander.publish(LandEvent::Verification {
                seq,
                green: status == VerificationStatus::Green,
            });
        }
        Ok(())
    }

    pub(super) async fn set_culprit(&self, seq: TrunkSeq, culprit: TrunkSeq) -> Result<()> {
        let repo = self.lander.repo();
        if let Some(mut record) = record_at(repo, seq).await? {
            record.culprit = Some(culprit);
            put_record(repo, &record).await?;
        }
        Ok(())
    }
}
