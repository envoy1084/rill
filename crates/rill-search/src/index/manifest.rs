//! Per-root manifests: the files of a tree and what content they hold.

use rill_types::{ContentRef, FileKind, ObjectHash, RepoPath, RepoPathBuf};
use serde::{Deserialize, Serialize};

/// One file of an indexed root.
///
/// The full [`ContentRef`] is kept rather than only its hash because inline
/// content lives nowhere but in the reference itself; the hash is derived
/// through [`ManifestEntry::content_hash`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: RepoPathBuf,
    pub content: ContentRef,
    pub kind: FileKind,
}

impl ManifestEntry {
    /// Dedup key of the content, matching the document table.
    pub fn content_hash(&self) -> ObjectHash {
        self.content.content_hash()
    }
}

/// The files under one root tree, sorted by path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootManifest {
    pub entries: Vec<ManifestEntry>,
}

impl RootManifest {
    /// The entry at `path`, if any.
    pub fn get(&self, path: &RepoPath) -> Option<&ManifestEntry> {
        self.entries
            .binary_search_by(|e| e.path.as_path().cmp(path))
            .ok()
            .map(|i| &self.entries[i])
    }

    /// Number of files.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True for an empty tree.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
