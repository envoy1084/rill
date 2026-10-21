//! Events the lander publishes.

use rill_types::{ChangeId, ChangeVersionHash, RepoPathBuf, TrunkSeq};
use serde::{Deserialize, Serialize};

use crate::queue::LandTicket;

/// Something that happened in the landing pipeline or post-land
/// verification. Delivered over a `tokio::sync::broadcast` channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LandEvent {
    /// A request entered the queue.
    Queued {
        ticket: LandTicket,
        change_id: ChangeId,
    },
    /// A version landed on trunk.
    Landed {
        seq: TrunkSeq,
        change_id: ChangeId,
        version: ChangeVersionHash,
    },
    /// A request was rejected because its rebase conflicted.
    Conflict {
        change_id: ChangeId,
        paths: Vec<RepoPathBuf>,
    },
    /// A request failed for a reason other than a conflict.
    Failed { change_id: ChangeId, reason: String },
    /// Changes stacked on a landed change may now be landed.
    DependentsReady {
        landed: ChangeId,
        dependents: Vec<ChangeId>,
    },
    /// Most rebases in `domain` conflict: it is served one request per
    /// batch until that drops.
    Throttled { domain: String, rate_permille: u32 },
    /// Post-land verification finished for a trunk position.
    Verification { seq: TrunkSeq, green: bool },
    /// Bisection identified the first red trunk position.
    Culprit { seq: TrunkSeq },
    /// A revert of `culprit` landed at `revert`.
    Reverted { culprit: TrunkSeq, revert: TrunkSeq },
}
