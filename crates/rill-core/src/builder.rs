//! Fluent construction of change versions.

use std::collections::BTreeMap;

use bytes::Bytes;
use rill_store::{ObjectStore, RepoStore};
use rill_types::objects::change::FileState;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, FileChange, FileKind, IntentId, Merge, Principal,
    Provenance, RepoPathBuf, SubtreeChange, Timestamp,
};

use crate::blobs::store_content;
use crate::error::Result;

enum Pending {
    Set(Bytes, FileKind),
    Change(FileChange),
}

/// Builds a [`ChangeVersion`], storing file content as it goes.
pub struct ChangeBuilder {
    change_id: ChangeId,
    parents: Vec<ChangeVersionHash>,
    predecessor: Option<ChangeVersionHash>,
    author: Principal,
    message: String,
    created_at: Timestamp,
    provenance: Provenance,
    intent: Option<IntentId>,
    subtree_changes: BTreeMap<RepoPathBuf, SubtreeChange>,
    files: BTreeMap<RepoPathBuf, Pending>,
}

impl ChangeBuilder {
    /// A new change by `author` with no parents.
    pub fn new(author: Principal) -> Self {
        ChangeBuilder {
            change_id: ChangeId::new(),
            parents: Vec::new(),
            predecessor: None,
            author,
            message: String::new(),
            created_at: Timestamp::now(),
            provenance: Provenance::default(),
            intent: None,
            subtree_changes: BTreeMap::new(),
            files: BTreeMap::new(),
        }
    }

    /// Use a specific change identity (for amends and rebases).
    pub fn change_id(mut self, id: ChangeId) -> Self {
        self.change_id = id;
        self
    }

    /// Add a parent; the first one added is the base for file changes.
    pub fn parent(mut self, parent: ChangeVersionHash) -> Self {
        self.parents.push(parent);
        self
    }

    /// Record the version this one amends.
    pub fn predecessor(mut self, predecessor: ChangeVersionHash) -> Self {
        self.predecessor = Some(predecessor);
        self
    }

    /// Set the message.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Set the creation time (defaults to now).
    pub fn created_at(mut self, at: Timestamp) -> Self {
        self.created_at = at;
        self
    }

    /// Set provenance.
    pub fn provenance(mut self, provenance: Provenance) -> Self {
        self.provenance = provenance;
        self
    }

    /// Link an intent.
    pub fn intent(mut self, intent: IntentId) -> Self {
        self.intent = Some(intent);
        self
    }

    /// Set a regular file's content.
    pub fn file(self, path: &str, bytes: impl Into<Bytes>) -> Result<Self> {
        self.file_kind(path, bytes, FileKind::Regular)
    }

    /// Set a file's content with an explicit kind.
    pub fn file_kind(
        mut self,
        path: &str,
        bytes: impl Into<Bytes>,
        kind: FileKind,
    ) -> Result<Self> {
        self.files
            .insert(RepoPathBuf::new(path)?, Pending::Set(bytes.into(), kind));
        Ok(self)
    }

    /// Delete a path.
    pub fn delete(mut self, path: &str) -> Result<Self> {
        self.files
            .insert(RepoPathBuf::new(path)?, Pending::Change(FileChange::Delete));
        Ok(self)
    }

    /// Record a conflict at a path.
    pub fn conflict(mut self, path: &str, terms: Merge<FileState>) -> Result<Self> {
        self.files.insert(
            RepoPathBuf::new(path)?,
            Pending::Change(FileChange::Conflict(terms)),
        );
        Ok(self)
    }

    /// Record an already-built file change.
    pub fn change(mut self, path: &str, change: FileChange) -> Result<Self> {
        self.files
            .insert(RepoPathBuf::new(path)?, Pending::Change(change));
        Ok(self)
    }

    /// Record a whole-directory change.
    pub fn subtree(mut self, path: &str, change: SubtreeChange) -> Result<Self> {
        self.subtree_changes.insert(RepoPathBuf::new(path)?, change);
        Ok(self)
    }

    /// Store content and produce the version (not yet stored).
    pub async fn build(self, objects: &dyn ObjectStore) -> Result<ChangeVersion> {
        let mut file_changes = BTreeMap::new();
        for (path, pending) in self.files {
            let change = match pending {
                Pending::Set(bytes, kind) => {
                    FileChange::set(store_content(objects, bytes).await?, kind)
                }
                Pending::Change(change) => change,
            };
            file_changes.insert(path, change);
        }
        Ok(ChangeVersion {
            change_id: self.change_id,
            parents: self.parents,
            predecessor: self.predecessor,
            subtree_changes: self.subtree_changes,
            file_changes,
            author: self.author,
            provenance: self.provenance,
            intent: self.intent,
            message: self.message,
            created_at: self.created_at,
            extra: BTreeMap::new(),
        })
    }

    /// Build and store the version in `repo`, returning its hash.
    pub async fn commit(self, repo: &RepoStore) -> Result<(ChangeVersionHash, ChangeVersion)> {
        let cv = self.build(repo.objects().as_ref()).await?;
        let hash = repo.put_change_version(&cv).await?;
        Ok((hash, cv))
    }
}
