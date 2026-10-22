//! Trunk status, verification and bisect records.

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

impl TryFrom<pb::VerificationResult> for VerificationRecord {
    type Error = Error;

    fn try_from(r: pb::VerificationResult) -> Result<Self> {
        Ok(VerificationRecord {
            seq: trunk_seq_from_proto(r.seq),
            status: VerificationStatus::from_proto(
                "verification_result.status",
                r.status.to_i32(),
            )?,
            culprit: r.culprit.map(trunk_seq_from_proto),
            reports: r
                .reports
                .into_iter()
                .map(VerificationReport::try_from)
                .collect::<Result<_>>()?,
        })
    }
}

/// A landing in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InFlightLanding {
    /// The ticket.
    pub ticket: LandTicket,
    /// The version being landed.
    pub version: ChangeVersionHash,
}

/// A snapshot of trunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrunkStatus {
    /// Head sequence.
    pub head_seq: TrunkSeq,
    /// Head version.
    pub head_hash: ChangeVersionHash,
    /// Landings in progress.
    pub in_flight: Vec<InFlightLanding>,
    /// Most recent landings, newest first.
    pub recent: Vec<LandEvent>,
    /// Latest sequence whose verification passed.
    pub last_verified_seq: Option<TrunkSeq>,
}

impl TryFrom<pb::GetTrunkStatusResponse> for TrunkStatus {
    type Error = Error;

    fn try_from(r: pb::GetTrunkStatusResponse) -> Result<Self> {
        Ok(TrunkStatus {
            head_seq: trunk_seq_from_proto(r.head_seq),
            head_hash: hash_from_proto("trunk_status.head_hash", &r.head_hash)?,
            in_flight: r
                .in_flight
                .into_iter()
                .map(|l| {
                    Ok(InFlightLanding {
                        ticket: required("in_flight.ticket", l.ticket)?.try_into()?,
                        version: hash_from_proto("in_flight.version", &l.version)?,
                    })
                })
                .collect::<Result<_>>()?,
            recent: r
                .recent
                .into_iter()
                .map(LandEvent::try_from)
                .collect::<Result<_>>()?,
            last_verified_seq: r.last_verified_seq.map(trunk_seq_from_proto),
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
