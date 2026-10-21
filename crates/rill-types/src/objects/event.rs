//! Events: what changed in a repository, recorded in the same transaction
//! as the change itself.

use serde::{Deserialize, Serialize};

use crate::ids::{ChangeId, IntentId, OperationId, WorkspaceId};
use crate::objects::change::ChangeVersionHash;
use crate::objects::refs::TrunkSeq;
use crate::objects::review::ReviewStatus;
use crate::principal::Principal;
use crate::time::Timestamp;

/// What happened. Every committed change to a repository's state writes
/// exactly one of these per change, in the same transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// The repository was created.
    RepoCreated { name: String },
    /// A ref was created, moved or deleted.
    RefMoved {
        name: String,
        old: Option<ChangeVersionHash>,
        new: Option<ChangeVersionHash>,
    },
    /// A change entered review.
    ReviewSubmitted {
        change: ChangeId,
        version: ChangeVersionHash,
    },
    /// A vote was cast or replaced.
    Voted {
        change: ChangeId,
        label: String,
        value: i8,
        version: ChangeVersionHash,
    },
    /// A comment was posted; `new_thread` when it opened the thread.
    Commented {
        change: ChangeId,
        thread: String,
        new_thread: bool,
    },
    /// A thread was resolved or reopened.
    ThreadResolved {
        change: ChangeId,
        thread: String,
        resolved: bool,
    },
    /// The change's declared stack changed.
    StackSet {
        change: ChangeId,
        stack: Vec<ChangeId>,
    },
    /// A workspace operation was applied.
    WorkspaceChanged {
        workspace: WorkspaceId,
        operation: OperationId,
    },
    /// A workspace was deleted.
    WorkspaceDeleted { workspace: WorkspaceId },
    /// An intent was created or updated.
    IntentChanged { intent: IntentId },
    /// A relationship tuple was added or removed.
    RelationChanged {
        object: String,
        relation: String,
        subject: String,
        added: bool,
    },
    /// A stored policy was replaced.
    PolicyChanged { name: String },
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

impl EventKind {
    /// A short stable name, for filters and logs.
    pub fn name(&self) -> &'static str {
        match self {
            EventKind::RepoCreated { .. } => "repo_created",
            EventKind::RefMoved { .. } => "ref_moved",
            EventKind::VersionStored { .. } => "version_stored",
            EventKind::ReviewSubmitted { .. } => "review_submitted",
            EventKind::Voted { .. } => "voted",
            EventKind::Commented { .. } => "commented",
            EventKind::ThreadResolved { .. } => "thread_resolved",
            EventKind::StackSet { .. } => "stack_set",
            EventKind::StatusChanged { .. } => "status_changed",
            EventKind::Landed { .. } => "landed",
            EventKind::WorkspaceChanged { .. } => "workspace_changed",
            EventKind::WorkspaceDeleted { .. } => "workspace_deleted",
            EventKind::IntentChanged { .. } => "intent_changed",
            EventKind::RelationChanged { .. } => "relation_changed",
            EventKind::PolicyChanged { .. } => "policy_changed",
        }
    }

    /// The change the event concerns, if any.
    pub fn change(&self) -> Option<ChangeId> {
        match self {
            EventKind::VersionStored { change, .. }
            | EventKind::ReviewSubmitted { change, .. }
            | EventKind::Voted { change, .. }
            | EventKind::Commented { change, .. }
            | EventKind::ThreadResolved { change, .. }
            | EventKind::StackSet { change, .. }
            | EventKind::StatusChanged { change, .. }
            | EventKind::Landed { change, .. } => Some(*change),
            _ => None,
        }
    }
}
