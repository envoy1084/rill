//! The [`GitSource`] abstraction and the data it yields.

use std::fmt;

use bytes::Bytes;
use rill_types::{FileKind, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// A git object id as lowercase hex (SHA-1 or SHA-256).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GitSha(String);

impl GitSha {
    /// Wrap a hex string, normalizing case.
    pub fn new(hex: impl AsRef<str>) -> Self {
        GitSha(hex.as_ref().to_ascii_lowercase())
    }

    /// Hex form.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Raw bytes, for storing on a change version.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Only constructed from valid hex by this crate's readers; an odd
        // or non-hex string would come from a corrupt import map.
        hex::decode(&self.0).unwrap_or_default()
    }
}

impl fmt::Debug for GitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GitSha({})", &self.0[..self.0.len().min(12)])
    }
}

impl fmt::Display for GitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Metadata of one git commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    pub sha: GitSha,
    /// Parents in git order; the first is the mainline parent.
    pub parents: Vec<GitSha>,
    pub author_name: String,
    pub author_email: String,
    /// Committer timestamp.
    pub committed_at: Timestamp,
    pub message: String,
}

/// One path changed by a commit relative to its first parent. Renames are
/// a delete plus a set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitChange {
    /// The path now holds `content`; for symlinks that is the target.
    Set {
        path: String,
        content: Bytes,
        kind: FileKind,
    },
    /// The path is gone.
    Delete { path: String },
}

impl GitChange {
    /// The affected path.
    pub fn path(&self) -> &str {
        match self {
            GitChange::Set { path, .. } | GitChange::Delete { path } => path,
        }
    }
}

/// Changes of one commit, plus anything the reader had to skip.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitChanges {
    pub changes: Vec<GitChange>,
    /// Human-readable notes about skipped entries (submodules).
    pub warnings: Vec<String>,
}

/// A readable git history. Implementations are synchronous; the importer
/// drives them from a blocking thread.
pub trait GitSource: Send + 'static {
    /// Resolve a ref name (`HEAD`, `main`, `refs/tags/v1`) or a full hex
    /// object id to a commit.
    fn resolve(&mut self, head_ref: &str) -> Result<GitSha>;

    /// Metadata of a commit.
    fn commit(&mut self, sha: &GitSha) -> Result<GitCommit>;

    /// Changes of a commit against its first parent (or the empty tree).
    fn changes(&mut self, sha: &GitSha) -> Result<CommitChanges>;
}
