//! Events: what changed in a repository, recorded in the same transaction
//! as the change itself.

use serde::{Deserialize, Serialize};

use crate::ids::ChangeId;
use crate::objects::change::ChangeVersionHash;
use crate::objects::refs::TrunkSeq;
use crate::objects::review::ReviewStatus;
use crate::principal::Principal;
use crate::time::Timestamp;

/// What happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// A change version was stored.
    VersionStored {
        version: ChangeVersionHash,
        change: ChangeId,
    },
    /// A change under review moved to a new status.
    StatusChanged {
        change: ChangeId,
        from: ReviewStatus,
        to: ReviewStatus,
    },
    /// A version landed on trunk.
    Landed {
        seq: TrunkSeq,
        version: ChangeVersionHash,
        change: ChangeId,
    },
}

/// One recorded event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub kind: EventKind,
    /// Who caused it.
    pub actor: Principal,
    pub at: Timestamp,
}

impl crate::Record for Event {
    const TAG: &'static str = "event";
}
