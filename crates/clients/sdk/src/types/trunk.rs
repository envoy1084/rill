//! Trunk status, verification, bisect and bulk-revert records.

use bytes::Bytes;
use rill_proto::convert::{
    hash_from_proto, opt_hash_from_proto, required, timestamp_from_proto, trunk_seq_from_proto,
};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersion, ChangeVersionHash, ObjectHash, Principal, Timestamp, TrunkSeq};

use super::repo::LandEvent;
use super::review::LandTicket;
use crate::error::{Error, Result};

/// Outcome of a verification target for a landed sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerificationStatus {
    /// Not started.
    Pending,
    /// In progress.
    Running,
    /// Passed.
    Passed,
    /// Failed.
    Failed,
    /// Failed, and a later bisect found an earlier culprit.
    Inherited,
    /// Not run.
    Skipped,
}

impl VerificationStatus {
    /// Wire encoding.
    pub fn to_proto(self) -> i32 {
        let v = match self {
            VerificationStatus::Pending => pb::VerificationStatus::Pending,
            VerificationStatus::Running => pb::VerificationStatus::Running,
            VerificationStatus::Passed => pb::VerificationStatus::Passed,
            VerificationStatus::Failed => pb::VerificationStatus::Failed,
            VerificationStatus::Inherited => pb::VerificationStatus::Inherited,
            VerificationStatus::Skipped => pb::VerificationStatus::Skipped,
        };
        v as i32
    }

    fn from_proto(field: &'static str, value: i32) -> Result<Self> {
        match rill_proto::convert::known::<pb::VerificationStatus>(value) {
            Ok(pb::VerificationStatus::Pending) => Ok(VerificationStatus::Pending),
            Ok(pb::VerificationStatus::Running) => Ok(VerificationStatus::Running),
            Ok(pb::VerificationStatus::Passed) => Ok(VerificationStatus::Passed),
            Ok(pb::VerificationStatus::Failed) => Ok(VerificationStatus::Failed),
            Ok(pb::VerificationStatus::Inherited) => Ok(VerificationStatus::Inherited),
            Ok(pb::VerificationStatus::Skipped) => Ok(VerificationStatus::Skipped),
            _ => Err(rill_proto::ConvertError::BadEnum { field, value }.into()),
        }
    }
}

/// One runner's report for one landed sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    /// Verification target or runner name.
    pub target: String,
    /// Outcome.
    pub status: VerificationStatus,
    /// Who reported it.
    pub reporter: Principal,
    /// When it was reported.
    pub reported_at: Timestamp,
    /// Free-form detail.
    pub details: Option<String>,
    /// Content-addressed log or artifact.
    pub artifact: Option<ObjectHash>,
}

impl TryFrom<pb::VerificationReport> for VerificationReport {
    type Error = Error;

    fn try_from(r: pb::VerificationReport) -> Result<Self> {
        Ok(VerificationReport {
            target: r.target,
            status: VerificationStatus::from_proto(
                "verification_report.status",
                r.status.to_i32(),
            )?,
            reporter: required("verification_report.reporter", r.reporter)?.try_into()?,
            reported_at: timestamp_from_proto(r.reported_at),
            details: r.details,
            artifact: opt_hash_from_proto("verification_report.artifact", r.artifact.as_ref())?,
        })
    }
}

/// Aggregate verification state of one landed sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationRecord {
    /// The sequence.
    pub seq: TrunkSeq,
    /// Aggregate outcome.
    pub status: VerificationStatus,
    /// The sequence that introduced the failure, when known.
    pub culprit: Option<TrunkSeq>,
    /// Per-target reports.
    pub reports: Vec<VerificationReport>,
}

impl TryFrom<pb::Verification> for VerificationRecord {
    type Error = Error;

    fn try_from(r: pb::Verification) -> Result<Self> {
        Ok(VerificationRecord {
            seq: trunk_seq_from_proto(r.position),
            status: VerificationStatus::from_proto("verification.status", r.status.to_i32())?,
            culprit: r.culprit.map(trunk_seq_from_proto),
            reports: r
                .reports
                .into_iter()
                .map(VerificationReport::try_from)
                .collect::<Result<_>>()?,
        })
    }
}

/// A snapshot of trunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrunkStatus {
    /// Head sequence.
    pub head_seq: TrunkSeq,
    /// Head version; `None` until something lands.
    pub head_hash: Option<ChangeVersionHash>,
    /// Land requests queued or rebasing, in serve order.
    pub in_flight: Vec<LandTicket>,
    /// Most recent landings, newest first.
    pub recent: Vec<LandEvent>,
    /// Latest sequence whose verification passed.
    pub last_verified_seq: Option<TrunkSeq>,
}

impl TrunkStatus {
    /// The status, with `recent` the landings fetched alongside it.
    pub(crate) fn from_parts(
        r: pb::GetTrunkStatusResponse,
        recent: Vec<LandEvent>,
    ) -> Result<Self> {
        Ok(TrunkStatus {
            head_seq: trunk_seq_from_proto(r.head_position),
            head_hash: match r.head_hash.is_empty() {
                true => None,
                false => Some(hash_from_proto("trunk_status.head_hash", &r.head_hash)?),
            },
            in_flight: r
                .in_flight
                .into_iter()
                .map(LandTicket::try_from)
                .collect::<Result<_>>()?,
            recent,
            last_verified_seq: r.last_verified_position.map(trunk_seq_from_proto),
        })
    }
}

/// A landing with its verification state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landing {
    /// The landing.
    pub event: LandEvent,
    /// Verification, if any has been recorded.
    pub verification: Option<VerificationRecord>,
}

/// A verification report to submit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewVerification {
    /// Verification target or runner name.
    pub target: String,
    /// Outcome.
    pub status: VerificationStatus,
    /// Free-form detail.
    pub details: Option<String>,
    /// Content-addressed log or artifact already uploaded.
    pub artifact: Option<ObjectHash>,
}

/// A revert landing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertResult {
    /// The landing of the revert.
    pub landing: LandEvent,
    /// The revert version.
    pub version: ChangeVersion,
}

/// Outcome of a bisect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BisectResult {
    /// The first bad sequence, if found.
    pub culprit: Option<TrunkSeq>,
    /// Sequences tested, with their outcomes.
    pub tested: Vec<VerificationRecord>,
}

/// Opaque derived data bytes.
pub type DerivedBytes = Bytes;

/// Which landings a bulk revert covers; name at least one origin.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RevertSelector {
    /// A session (`ss_…`).
    pub session: Option<String>,
    /// A model.
    pub model: Option<String>,
    /// A sponsor, by name (`ann`) or as shown (`agent:bot`).
    pub sponsor: Option<String>,
    /// An author.
    pub author: Option<String>,
    /// An intent.
    pub intent: Option<rill_types::IntentId>,
    /// Positions after this one.
    pub since: Option<TrunkSeq>,
    /// Positions up to this one.
    pub until: Option<TrunkSeq>,
}

impl RevertSelector {
    pub(crate) fn to_proto(&self, repo: &str) -> pb::RevertSelector {
        pb::RevertSelector {
            session: self.session.clone(),
            model: self.model.clone(),
            sponsor: self.sponsor.clone(),
            author: self.author.clone(),
            intent: self.intent.map(|i| crate::names::intent(repo, &i)),
            since: self.since.map(TrunkSeq::get),
            until: self.until.map(TrunkSeq::get),
        }
    }
}
