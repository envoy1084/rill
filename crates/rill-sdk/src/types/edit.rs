//! Inputs to the history-writing RPCs.

use bytes::Bytes;
use rill_proto::convert::id_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, ContentRef, FileKind, IntentId, Provenance,
    RepoPathBuf,
};

use super::root::Root;

/// What an edit is applied on top of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditBase {
    /// A point in history; the edit creates a new change on top of it.
    Root(Root),
    /// The current version of a change.
    Change(ChangeId),
}

impl EditBase {
    /// Wire encoding.
    pub fn to_proto(&self) -> pb::EditBase {
        let base = match self {
            EditBase::Root(r) => pb::edit_base::Base::Root(r.to_proto()),
            EditBase::Change(id) => pb::edit_base::Base::ChangeId(id_to_proto(id)),
        };
        pb::EditBase { base: Some(base) }
    }
}

impl From<Root> for EditBase {
    fn from(r: Root) -> Self {
        EditBase::Root(r)
    }
}

/// One edit to apply to a path. Content is given as bytes; the SDK inlines
/// small files and uploads large ones as blobs before sending the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Replace or create the path with these bytes.
    Set {
        /// The path.
        path: RepoPathBuf,
        /// New content.
        bytes: Bytes,
        /// File kind.
        kind: FileKind,
    },
    /// Remove the path.
    Delete {
        /// The path.
        path: RepoPathBuf,
    },
    /// Apply a unified diff to the base content.
    Patch {
        /// The path.
        path: RepoPathBuf,
        /// Unified diff text.
        unified: String,
    },
}

impl Edit {
    /// Set a regular file.
    pub fn set(path: RepoPathBuf, bytes: impl Into<Bytes>) -> Self {
        Edit::Set {
            path,
            bytes: bytes.into(),
            kind: FileKind::Regular,
        }
    }

    /// Delete a path.
    pub fn delete(path: RepoPathBuf) -> Self {
        Edit::Delete { path }
    }

    /// The edited path.
    pub fn path(&self) -> &RepoPathBuf {
        match self {
            Edit::Set { path, .. } | Edit::Delete { path } | Edit::Patch { path, .. } => path,
        }
    }
}

/// Metadata for a change produced by `apply_edits`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyOptions {
    /// Change message.
    pub message: String,
    /// Intent the change belongs to.
    pub intent: Option<IntentId>,
    /// Who and what produced it.
    pub provenance: Provenance,
    /// Amend this change rather than creating a new one.
    pub change_id: Option<ChangeId>,
}

impl ApplyOptions {
    /// Options with only a message.
    pub fn message(message: impl Into<String>) -> Self {
        ApplyOptions {
            message: message.into(),
            ..Default::default()
        }
    }

    /// Amend an existing change.
    pub fn amending(mut self, change_id: ChangeId) -> Self {
        self.change_id = Some(change_id);
        self
    }

    /// Attach an intent.
    pub fn with_intent(mut self, intent: IntentId) -> Self {
        self.intent = Some(intent);
        self
    }

    /// Attach provenance.
    pub fn with_provenance(mut self, provenance: Provenance) -> Self {
        self.provenance = provenance;
        self
    }
}

/// Metadata for a merge change.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergeOptions {
    /// Change message.
    pub message: String,
    /// Intent the change belongs to.
    pub intent: Option<IntentId>,
    /// Who and what produced it.
    pub provenance: Provenance,
}

/// How to resolve one conflicted path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The path takes this content.
    Set {
        /// The path.
        path: RepoPathBuf,
        /// Resolved content.
        bytes: Bytes,
        /// File kind.
        kind: FileKind,
    },
    /// The path is removed.
    Delete {
        /// The path.
        path: RepoPathBuf,
    },
}

/// A version returned by a history-writing RPC, with its hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionResult {
    /// Hash of the new version.
    pub hash: ChangeVersionHash,
    /// The new version.
    pub version: ChangeVersion,
}

impl VersionResult {
    /// Build from the hash and version fields shared by every write response.
    pub(crate) fn from_parts(
        hash: &[u8],
        version: Option<pb::ChangeVersion>,
    ) -> crate::error::Result<Self> {
        Ok(VersionResult {
            hash: rill_proto::convert::hash_from_proto("response.hash", hash)?,
            version: rill_proto::convert::required("response.version", version)?.try_into()?,
        })
    }

    /// Paths left in conflict by this version.
    pub fn conflicted_paths(&self) -> Vec<RepoPathBuf> {
        self.version.conflicted_paths().cloned().collect()
    }
}

/// Wire encoding of resolved content and kind.
pub(crate) fn set_content_proto(content: ContentRef, kind: FileKind) -> pb::SetContent {
    pb::SetContent {
        content: Some(pb::ContentRef::from(content)),
        kind: pb::FileKind::from(kind) as i32,
        copy_from: None,
    }
}
