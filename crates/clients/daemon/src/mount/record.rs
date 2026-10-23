//! A mount's persisted description.

use std::path::PathBuf;

use rill_types::{ChangeId, ChangeVersionHash, SparseProfile, TreeHash, WorkspaceId};
use serde::{Deserialize, Serialize};

/// How a mount presents its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MountMode {
    /// Files on disk, edits found by a watcher.
    Eager,
    /// A virtual filesystem with contents fetched on first read.
    Lazy,
    /// A read-only virtual filesystem of one version with a scratch
    /// overlay that is never synced.
    Snapshot,
}

/// Everything the daemon keeps about a mount besides its overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountRecord {
    /// Stable id, the `{mount}` of `mounts/{mount}`.
    pub id: String,
    /// Absolute directory.
    pub path: PathBuf,
    /// Server endpoint.
    pub endpoint: String,
    /// Repository name.
    pub repo: String,
    /// How files are presented.
    pub mode: MountMode,
    /// The server workspace edits sync to.
    pub workspace: Option<WorkspaceId>,
    /// The revision the base was taken from.
    pub base: String,
    /// The change version the base is; `None` for an empty repository.
    pub base_version: Option<ChangeVersionHash>,
    /// The base's root tree.
    pub base_tree: TreeHash,
    /// The change commits amend.
    pub working_change: Option<ChangeId>,
    /// Directories included.
    pub sparse: SparseProfile,
    /// The workspace's etag after the daemon's last push; `None` forces
    /// the next push to rebuild the snapshot from the base.
    pub etag: Option<String>,
    /// Overlay sequence pushed so far.
    pub synced: u64,
}

impl MountRecord {
    /// The resource name, `mounts/{id}`.
    pub fn name(&self) -> String {
        format!("mounts/{}", self.id)
    }
}
