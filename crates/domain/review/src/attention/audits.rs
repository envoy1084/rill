//! Sampled human audits of changes that landed on agent review alone.
//!
//! A sample of those landings (the intent's `human_sample_permille`, or
//! the repository default, weighted up with risk) is assigned after
//! landing to the owners of what it touched. A failed audit counts as an
//! escape for the agents that approved it.

use rill_types::{ChangeId, ChangeVersionHash, Principal, RiskClass, Timestamp, TrunkSeq};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::lifecycle::MAX_WRITE_ATTEMPTS;
use crate::service::ReviewService;

/// Landings audited per thousand when no intent says otherwise.
pub const DEFAULT_SAMPLE_PERMILLE: u32 = 20;
/// Pending audits kept in the index.
const MAX_PENDING: usize = 10_000;

/// Where an audit stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditStatus {
    Pending,
    Passed,
    Failed,
}

/// One sampled landing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Audit {
    pub seq: TrunkSeq,
    pub change: ChangeId,
    pub version: ChangeVersionHash,
    /// Who should look: `user:…`, `team:…`, or `owners` (the repository's
    /// `owner` relation) when no `OWNERS.toml` covers it.
    pub assignees: Vec<String>,
    pub risk: RiskClass,
    pub status: AuditStatus,
    pub sampled_at: Timestamp,
    pub decided_by: Option<Principal>,
    pub decided_at: Option<Timestamp>,
    pub note: String,
}

/// Whether the landing at `seq` is sampled at `permille`, weighted by
/// risk: twice for normal, four times for sensitive and above.
pub fn sampled(seq: TrunkSeq, permille: u32, risk: RiskClass) -> bool {
    let weight = match risk {
        RiskClass::Trivial | RiskClass::Low => 1,
        RiskClass::Normal => 2,
        RiskClass::Sensitive | RiskClass::Critical => 4,
    };
    let rate = (permille * weight).min(1000);
    let h = blake3::hash(format!("audit:{}", seq.0).as_bytes());
    let draw = u32::from_le_bytes(h.as_bytes()[..4].try_into().expect("four bytes")) % 1000;
    draw < rate
}

fn audit_key(seq: TrunkSeq) -> String {
    format!("audit:{}", seq.0)
}

const PENDING_KEY: &str = "audits-pending";

fn codec(e: serde_json::Error) -> Error {
    Error::Store(rill_store::Error::Codec(format!("audits: {e}")))
}

impl ReviewService {
    /// The audit of the landing at `seq`, if it was sampled.
    pub async fn audit(&self, seq: TrunkSeq) -> Result<Option<Audit>> {
        match self.repo.get_misc(&audit_key(seq)).await? {
            Some(b) => Ok(Some(serde_json::from_slice(&b).map_err(codec)?)),
            None => Ok(None),
        }
    }

    /// Audits still waiting, oldest first.
    pub async fn pending_audits(&self) -> Result<Vec<Audit>> {
        let seqs: Vec<u64> = match self.repo.get_misc(PENDING_KEY).await? {
            Some(b) => serde_json::from_slice(&b).map_err(codec)?,
            None => Vec::new(),
        };
        let mut out = Vec::with_capacity(seqs.len());
        for seq in seqs {
            if let Some(a) = self.audit(TrunkSeq(seq)).await? {
                if a.status == AuditStatus::Pending {
                    out.push(a);
                }
            }
        }
        Ok(out)
    }

    async fn update_pending(&self, f: impl Fn(&mut Vec<u64>)) -> Result<()> {
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let current = self.repo.get_misc(PENDING_KEY).await?;
            let mut seqs: Vec<u64> = match &current {
                Some(b) => serde_json::from_slice(b).map_err(codec)?,
                None => Vec::new(),
            };
            f(&mut seqs);
            if seqs.len() > MAX_PENDING {
                seqs.drain(..seqs.len() - MAX_PENDING);
            }
            let value = serde_json::to_vec(&seqs).map_err(codec)?;
            match self
                .repo
                .set_misc_cas(PENDING_KEY, current, value.into())
                .await
            {
                Ok(()) => return Ok(()),
                Err(e) if e.is_conflict() => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::Store(rill_store::Error::Conflict(
            "the audit index kept changing".into(),
        )))
    }

    /// Record a new audit; false when the landing already had one.
    pub async fn open_audit(&self, audit: &Audit) -> Result<bool> {
        let value = serde_json::to_vec(audit).map_err(codec)?;
        match self
            .repo
            .set_misc_cas(&audit_key(audit.seq), None, value.into())
            .await
        {
            Ok(()) => {}
            Err(e) if e.is_conflict() => return Ok(false),
            Err(e) => return Err(e.into()),
        }
        let seq = audit.seq.0;
        self.update_pending(|s| {
            if !s.contains(&seq) {
                s.push(seq)
            }
        })
        .await?;
        Ok(true)
    }

    /// Decide a pending audit.
    pub async fn decide_audit(
        &self,
        seq: TrunkSeq,
        passed: bool,
        by: &Principal,
        note: &str,
    ) -> Result<Audit> {
        let key = audit_key(seq);
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let current = self
                .repo
                .get_misc(&key)
                .await?
                .ok_or_else(|| Error::BadResult(format!("trunk@{} was not sampled", seq.0)))?;
            let mut audit: Audit = serde_json::from_slice(&current).map_err(codec)?;
            if audit.status != AuditStatus::Pending {
                return Err(Error::BadResult(format!(
                    "the audit of trunk@{} was already decided",
                    seq.0
                )));
            }
            audit.status = if passed {
                AuditStatus::Passed
            } else {
                AuditStatus::Failed
            };
            audit.decided_by = Some(by.clone());
            audit.decided_at = Some(Timestamp::now());
            audit.note = note.to_string();
            let value = serde_json::to_vec(&audit).map_err(codec)?;
            match self
                .repo
                .set_misc_cas(&key, Some(current), value.into())
                .await
            {
                Ok(()) => {
                    self.update_pending(|s| s.retain(|n| *n != seq.0)).await?;
                    return Ok(audit);
                }
                Err(e) if e.is_conflict() => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::Store(rill_store::Error::Conflict(
            "the audit kept changing".into(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_follows_the_rate_and_weighs_risk() {
        let count = |permille, risk| {
            (0..10_000)
                .filter(|n| sampled(TrunkSeq(*n), permille, risk))
                .count()
        };
        let low = count(20, RiskClass::Low);
        assert!((150..=250).contains(&low), "{low}");
        let sensitive = count(20, RiskClass::Sensitive);
        assert!((650..=950).contains(&sensitive), "{sensitive}");
        assert_eq!(count(0, RiskClass::Critical), 0);
        assert_eq!(count(1000, RiskClass::Low), 10_000);
    }
}
