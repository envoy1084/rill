//! Landing records: what landed where, written with the trunk move.

use serde::{Deserialize, Serialize};

use crate::ids::ChangeId;
use crate::objects::change::ChangeVersionHash;
use crate::objects::intent::RiskClass;
use crate::objects::refs::TrunkSeq;
use crate::principal::Principal;
use crate::time::Timestamp;

/// One landed version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Landing {
    pub seq: TrunkSeq,
    pub change_id: ChangeId,
    /// The version that landed (after any rebase the lander performed).
    pub version: ChangeVersionHash,
    /// The version that was requested, when the lander rebased it.
    pub requested_version: ChangeVersionHash,
    pub requester: Principal,
    pub risk: RiskClass,
    pub landed_at: Timestamp,
    /// BLAKE3 of the submit policy that allowed the landing, checked in the
    /// landing's own transaction; `None` for a change landed without review.
    pub policy: Option<[u8; 32]>,
    /// The votes that policy saw.
    pub approvals: Vec<crate::objects::review::LabelVote>,
}

impl crate::Record for Landing {
    const TAG: &'static str = "landing";
}
