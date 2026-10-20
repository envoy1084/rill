//! Working-copy status: the manifest compared with the filesystem.

use std::path::Path;

use rill_types::RepoPathBuf;

use super::fs::{local_hash, read_entry, stat};
use super::meta::{Manifest, ManifestEntry};
use super::walk::{to_fs_path, walk_files};
use crate::error::Result;

/// How a path differs from the checkout's base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalChangeKind {
    /// Present locally, absent in the base.
    Added,
    /// Present in both with different content or kind.
    Modified,
    /// Absent locally, present in the base.
    Deleted,
    /// Holds unresolved conflict markers from an update.
    Conflict,
}

/// One path that differs from the base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalChange {
    /// The path.
    pub path: RepoPathBuf,
    /// How it differs.
    pub kind: LocalChangeKind,
}

/// Compare every tracked path and every file on disk with the manifest.
/// Blocking; run it off the async runtime.
pub fn compute_status(root: &Path, manifest: &Manifest) -> Result<Vec<LocalChange>> {
    let mut changes = Vec::new();
    for (path, entry) in manifest {
        let fs_path = to_fs_path(root, path);
        let kind = if entry.conflict {
            Some(LocalChangeKind::Conflict)
        } else {
            tracked_change(&fs_path, entry)?
        };
        if let Some(kind) = kind {
            changes.push(LocalChange {
                path: path.clone(),
                kind,
            });
        }
    }
    for (path, _) in walk_files(root)? {
        if !manifest.contains_key(&path) {
            changes.push(LocalChange {
                path,
                kind: LocalChangeKind::Added,
            });
        }
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

/// Whether a tracked file changed. Size and mtime matching the manifest
/// is taken as unchanged; anything else is confirmed by hashing.
fn tracked_change(fs_path: &Path, entry: &ManifestEntry) -> Result<Option<LocalChangeKind>> {
    let Some(st) = stat(fs_path)? else {
        return Ok(Some(LocalChangeKind::Deleted));
    };
    let kind_matches = st.is_symlink == (entry.kind == rill_types::FileKind::Symlink);
    if kind_matches && st.size == entry.size && st.mtime_ms == entry.mtime_ms {
        return Ok(None);
    }
    let (bytes, kind) = read_entry(fs_path)?;
    if kind == entry.kind && local_hash(&bytes) == entry.local_hash {
        return Ok(None);
    }
    Ok(Some(LocalChangeKind::Modified))
}
