//! Workspaces: a principal's in-progress state on a repository.

use serde::{Deserialize, Serialize};

use crate::ids::{ChangeId, OperationId, RepoId, WorkspaceId};
use crate::objects::change::ChangeVersionHash;
use crate::objects::tree::TreeHash;
use crate::path::RepoPathBuf;
use crate::principal::Principal;
use crate::time::Timestamp;

/// Which directories a workspace materializes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SparseProfile {
    /// Directory prefixes to include; empty means the whole repo.
    pub include: Vec<RepoPathBuf>,
    /// Directory prefixes to exclude even if under an include.
    pub exclude: Vec<RepoPathBuf>,
}

impl SparseProfile {
    pub fn full() -> Self {
        SparseProfile::default()
    }

    pub fn is_full(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    pub fn contains(&self, path: &crate::path::RepoPath) -> bool {
        if self.exclude.iter().any(|e| path.starts_with(e)) {
            return false;
        }
        self.include.is_empty() || self.include.iter().any(|i| path.starts_with(i))
    }
}

/// A workspace record. Mutable; every mutation is an [`Operation`] in the
/// workspace's log so it can be undone and audited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub repo: RepoId,
    pub owner: Principal,
    pub name: String,
    /// The change version the working copy is based on (the parent of the
    /// in-progress change).
    pub base: ChangeVersionHash,
    /// Stable id of the in-progress change.
    pub working_change: ChangeId,
    /// Latest uploaded snapshot of the working copy, if any.
    pub snapshot: Option<TreeHash>,
    pub sparse: SparseProfile,
    /// Latest operation applied to this workspace.
    pub head_op: Option<OperationId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationKind {
    Create,
    Snapshot {
        tree: TreeHash,
    },
    NewChange {
        change: ChangeId,
        base: ChangeVersionHash,
    },
    Describe {
        version: ChangeVersionHash,
    },
    Rebase {
        from: ChangeVersionHash,
        to: ChangeVersionHash,
    },
    Resolve {
        version: ChangeVersionHash,
    },
    SetSparse {
        profile: SparseProfile,
    },
    Handoff {
        to: Principal,
    },
    Undo {
        undone: OperationId,
    },
    Other {
        description: String,
    },
}

/// One entry in a workspace's operation log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub id: OperationId,
    pub workspace: WorkspaceId,
    pub parent: Option<OperationId>,
    pub actor: Principal,
    pub kind: OperationKind,
    /// Workspace state after this operation, so undo is a pointer move.
    pub resulting_base: ChangeVersionHash,
    pub resulting_snapshot: Option<TreeHash>,
    pub at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_profile_semantics() {
        let p = SparseProfile {
            include: vec![RepoPathBuf::new("src").unwrap()],
            exclude: vec![RepoPathBuf::new("src/vendor").unwrap()],
        };
        assert!(p.contains(&RepoPathBuf::new("src/lib.rs").unwrap()));
        assert!(!p.contains(&RepoPathBuf::new("src/vendor/x").unwrap()));
        assert!(!p.contains(&RepoPathBuf::new("docs/x").unwrap()));
        assert!(SparseProfile::full().contains(&RepoPathBuf::new("docs/x").unwrap()));
    }
}

impl crate::Record for Workspace {
    const TAG: &'static str = "workspace";
}

impl crate::Record for Operation {
    const TAG: &'static str = "operation";
}
