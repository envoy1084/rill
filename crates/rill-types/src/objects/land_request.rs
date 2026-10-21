//! Requests to land a change, and where each one is.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::ChangeId;
use crate::objects::change::ChangeVersionHash;
use crate::objects::intent::RiskClass;
use crate::objects::refs::TrunkSeq;
use crate::path::RepoPathBuf;
use crate::principal::{Principal, PrincipalKind};
use crate::time::Timestamp;

/// How much pre-land verification a request gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerifyMode {
    /// Land without verifying; post-land verification still runs.
    None,
    /// Verify only what the change affects. The lander passes this
    /// through to the verifier as a hint; today both modes call the same
    /// verifier.
    #[default]
    Affected,
    /// Run the full verification plan.
    Full,
}

/// Which queue a request waits in; earlier lanes are served first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    /// Reverts and fixes for a red trunk.
    Emergency,
    /// Changes requested by humans.
    Human,
    /// Changes requested by agents and services.
    Agent,
    /// Large refactors, codemods and imports.
    Bulk,
}

impl Lane {
    /// Every lane, most urgent first.
    pub const ALL: [Lane; 4] = [Lane::Emergency, Lane::Human, Lane::Agent, Lane::Bulk];

    /// The lane a request by `requester` goes to by default.
    pub fn for_requester(requester: &Principal) -> Lane {
        match requester.kind {
            PrincipalKind::Human => Lane::Human,
            PrincipalKind::Agent | PrincipalKind::Service => Lane::Agent,
        }
    }

    pub fn rank(self) -> u8 {
        self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            Lane::Emergency => "emergency",
            Lane::Human => "human",
            Lane::Agent => "agent",
            Lane::Bulk => "bulk",
        }
    }
}

/// A request to land one version of a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandRequest {
    pub change_id: ChangeId,
    pub version: ChangeVersionHash,
    pub requester: Principal,
    pub risk: RiskClass,
    pub requested_at: Timestamp,
    pub lane: Lane,
    /// Overrides the policy's verification mode for this request; used by
    /// reverts, which land without pre-land verification.
    pub verify_override: Option<VerifyMode>,
}

impl LandRequest {
    /// A request for `version` of `change_id` by `requester` at `risk`,
    /// in the requester's lane, timestamped now.
    pub fn new(
        change_id: ChangeId,
        version: ChangeVersionHash,
        requester: Principal,
        risk: RiskClass,
    ) -> Self {
        LandRequest {
            change_id,
            version,
            lane: Lane::for_requester(&requester),
            requester,
            risk,
            requested_at: Timestamp::now(),
            verify_override: None,
        }
    }

    /// The same request in `lane`.
    pub fn in_lane(mut self, lane: Lane) -> Self {
        self.lane = lane;
        self
    }
}

/// Handle to a land request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LandTicket(pub u64);

impl fmt::Display for LandTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "land#{}", self.0)
    }
}

/// Where a request is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LandStatus {
    Queued,
    Verifying,
    Landing,
    Landed {
        seq: TrunkSeq,
    },
    Failed {
        reason: String,
    },
    Conflict {
        paths: Vec<RepoPathBuf>,
    },
    Returned {
        reason: String,
    },
    /// Taken out of the queue before landing because what allowed it no
    /// longer holds (votes withdrawn, a new version, policy).
    Ejected {
        reason: String,
    },
}

impl LandStatus {
    /// True once the request will not change state again.
    pub fn is_terminal(&self) -> bool {
        !matches!(
            self,
            LandStatus::Queued | LandStatus::Verifying | LandStatus::Landing
        )
    }

    /// A short name for the status.
    pub fn name(&self) -> &'static str {
        match self {
            LandStatus::Queued => "queued",
            LandStatus::Verifying => "verifying",
            LandStatus::Landing => "landing",
            LandStatus::Landed { .. } => "landed",
            LandStatus::Failed { .. } => "failed",
            LandStatus::Conflict { .. } => "conflict",
            LandStatus::Returned { .. } => "returned",
            LandStatus::Ejected { .. } => "ejected",
        }
    }
}

/// A stored land request and where it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandRequestRecord {
    pub ticket: LandTicket,
    pub request: LandRequest,
    pub status: LandStatus,
    pub updated_at: Timestamp,
}

impl crate::Record for LandRequestRecord {
    const TAG: &'static str = "landreq";
}
