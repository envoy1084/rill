//! The persisted layers.

use rill_types::{FileKind, ObjectHash, RepoPathBuf, TreeHash};
use serde::{Deserialize, Serialize};

/// Deltas chained on one base before a new base is written.
pub const MAX_DELTA_DEPTH: u32 = 32;

/// One file of a root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathEntry {
    pub path: RepoPathBuf,
    /// The content's hash, which keys it in the segments.
    pub hash: ObjectHash,
    pub kind: FileKind,
}

/// What a delta does to one path: sets it, or removes it.
pub type PathChange = (RepoPathBuf, Option<(ObjectHash, FileKind)>);

/// The paths of one root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathLayer {
    /// Every file, sorted by path.
    Base(Vec<PathEntry>),
    /// The parent root's files with `changes` applied; `depth` deltas
    /// above the nearest base.
    Delta {
        parent: TreeHash,
        depth: u32,
        changes: Vec<PathChange>,
    },
}
