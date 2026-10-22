//! Change versions: the write-ahead-log entries that make up history.

use std::collections::BTreeMap;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::encoding::{Encodable, Schema};
use crate::hash::{ObjectHash, ObjectKind};
use crate::ids::{ChangeId, IntentId};
use crate::merge::Merge;
use crate::objects::blob::ContentRef;
use crate::objects::refs::TrunkSeq;
use crate::path::RepoPathBuf;
use crate::principal::Principal;
use crate::time::Timestamp;

/// Hash of a [`ChangeVersion`].
pub type ChangeVersionHash = ObjectHash;

/// How a file is stored and executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FileKind {
    Regular,
    Executable,
    Symlink,
}

impl FileKind {
    pub fn is_executable(self) -> bool {
        self == FileKind::Executable
    }
}

/// A file's state on one side of a merge: `None` means absent.
pub type FileState = Option<(ContentRef, FileKind)>;

/// What happened to one path, relative to the first parent.
///
/// There is no add/modify distinction: both are `Set`. A deleted path may be
/// a prefix of set paths (a file replaced by a directory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileChange {
    /// The path now has this content.
    Set {
        content: ContentRef,
        kind: FileKind,
        /// Explicit copy source, pinned to a parent version; renames are a
        /// copy plus a delete of the source.
        #[serde(default)]
        copy_from: Option<CopySource>,
    },
    /// The path no longer exists.
    Delete,
    /// The path is in conflict. Each term is the file's state on that side.
    Conflict(Merge<FileState>),
}

impl FileChange {
    pub fn set(content: ContentRef, kind: FileKind) -> Self {
        FileChange::Set {
            content,
            kind,
            copy_from: None,
        }
    }

    pub fn is_conflict(&self) -> bool {
        matches!(self, FileChange::Conflict(_))
    }

    /// The file state this change produces, or `None` if it is a conflict.
    pub fn resolved_state(&self) -> Option<FileState> {
        match self {
            FileChange::Set { content, kind, .. } => Some(Some((content.clone(), *kind))),
            FileChange::Delete => Some(None),
            FileChange::Conflict(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CopySource {
    pub path: RepoPathBuf,
    pub version: ChangeVersionHash,
}

/// A whole-directory operation, applied before file changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubtreeChange {
    /// Replace the directory at the key path with the directory at
    /// `from_path` as of `from_version` (shallow: the tree hash is reused).
    Copy {
        from_path: RepoPathBuf,
        from_version: ChangeVersionHash,
    },
    /// Remove the whole directory at the key path.
    Delete,
}

/// Who and what produced a change.
///
/// Provenance is signed into the change version, so it survives rewrites and
/// can be queried; it is never a commit-message convention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Provenance {
    /// Agent role or tool name (`claude-code`, `reviewer`, `lander`).
    #[serde(default)]
    pub tool: Option<String>,
    /// Model identifier for agent-authored changes.
    #[serde(default)]
    pub model: Option<String>,
    /// Session or run identifier that links to a transcript.
    #[serde(default)]
    pub session: Option<String>,
    /// Reference to the prompt or spec that drove the change.
    #[serde(default)]
    pub prompt_ref: Option<String>,
    /// Human who sponsored or delegated the work, if the author is an agent.
    #[serde(default)]
    pub sponsor: Option<Principal>,
    /// For resolutions: which stage of the conflict pipeline produced it.
    #[serde(default)]
    pub resolved_by: Option<String>,
    /// Free-form, forward-compatible extras.
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

/// A detached signature over a change version's hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    pub signer: Principal,
    /// Compressed P-256 public key, 33 bytes.
    pub public_key: Bytes,
    /// ECDSA P-256 signature over the object hash, 64 bytes (`r || s`).
    pub signature: Bytes,
    pub signed_at: Timestamp,
}

/// One concrete version of a change.
///
/// The hash of this object is the version identity; `change_id` is the stable
/// identity across versions. Like a Bonsai changeset, it stores only what
/// changed relative to `parents[0]`; the full tree is derived data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeVersion {
    pub change_id: ChangeId,
    pub parents: Vec<ChangeVersionHash>,
    /// The version this one amends or rebases, for walking a change's
    /// evolution.
    #[serde(default)]
    pub predecessor: Option<ChangeVersionHash>,
    #[serde(default)]
    pub subtree_changes: BTreeMap<RepoPathBuf, SubtreeChange>,
    pub file_changes: BTreeMap<RepoPathBuf, FileChange>,
    pub author: Principal,
    #[serde(default)]
    pub provenance: Provenance,
    #[serde(default)]
    pub intent: Option<IntentId>,
    pub message: String,
    pub created_at: Timestamp,
    #[serde(default)]
    pub extra: BTreeMap<String, Bytes>,
}

impl ChangeVersion {
    pub fn is_root(&self) -> bool {
        self.parents.is_empty()
    }

    pub fn first_parent(&self) -> Option<ChangeVersionHash> {
        self.parents.first().copied()
    }

    pub fn has_conflicts(&self) -> bool {
        self.file_changes.values().any(FileChange::is_conflict)
    }

    pub fn conflicted_paths(&self) -> impl Iterator<Item = &RepoPathBuf> {
        self.file_changes
            .iter()
            .filter(|(_, c)| c.is_conflict())
            .map(|(p, _)| p)
    }

    /// First line of the message.
    pub fn summary(&self) -> &str {
        self.message.lines().next().unwrap_or("")
    }

    /// Hash of this version. Signatures are not part of the hashed object;
    /// they are stored alongside it.
    pub fn version_hash(&self) -> ChangeVersionHash {
        self.hash()
    }
}

impl Encodable for ChangeVersion {
    const KIND: ObjectKind = ObjectKind::ChangeVersion;
    const SCHEMA: Schema = 1;
    const NAME: &'static str = "ChangeVersion";
}

/// Metadata-only projection of a change version, so that listing history
/// never loads a file-change map that may name a million paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeInfo {
    pub hash: ChangeVersionHash,
    pub change_id: ChangeId,
    pub parents: Vec<ChangeVersionHash>,
    pub predecessor: Option<ChangeVersionHash>,
    pub author: Principal,
    pub provenance: Provenance,
    pub intent: Option<IntentId>,
    pub message: String,
    pub created_at: Timestamp,
    pub files_changed: u64,
    pub has_conflicts: bool,
    /// Set once the version has landed on trunk.
    pub trunk_seq: Option<TrunkSeq>,
}

impl ChangeInfo {
    pub fn from_version(cv: &ChangeVersion, trunk_seq: Option<TrunkSeq>) -> Self {
        ChangeInfo {
            hash: cv.hash(),
            change_id: cv.change_id,
            parents: cv.parents.clone(),
            predecessor: cv.predecessor,
            author: cv.author.clone(),
            provenance: cv.provenance.clone(),
            intent: cv.intent,
            message: cv.message.clone(),
            created_at: cv.created_at,
            files_changed: cv.file_changes.len() as u64,
            has_conflicts: cv.has_conflicts(),
            trunk_seq,
        }
    }

    pub fn summary(&self) -> &str {
        self.message.lines().next().unwrap_or("")
    }
}

impl crate::Record for ChangeInfo {
    const TAG: &'static str = "change_info";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ChangeVersion {
        let mut file_changes = BTreeMap::new();
        file_changes.insert(
            RepoPathBuf::new("src/lib.rs").unwrap(),
            FileChange::set(
                ContentRef::inline(b"fn main(){}".to_vec()),
                FileKind::Regular,
            ),
        );
        ChangeVersion {
            change_id: ChangeId::derived("test", "a"),
            parents: vec![],
            predecessor: None,
            subtree_changes: BTreeMap::new(),
            file_changes,
            author: Principal::human("alice"),
            provenance: Provenance::default(),
            intent: None,
            message: "init\n\nbody".into(),
            created_at: Timestamp::from_millis(0),
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn encode_roundtrip() {
        let cv = sample();
        let bytes = cv.encode();
        assert_eq!(ChangeVersion::decode(&bytes).unwrap(), cv);
        assert_eq!(cv.summary(), "init");
    }

    #[test]
    fn hash_changes_with_content() {
        let a = sample();
        let mut b = sample();
        b.message.push('!');
        assert_ne!(a.hash(), b.hash());
    }
}
