//! Collecting the objects a response reveals and signing a grant for them.

use rill_policy::grants::GrantEntry;
use rill_policy::PathFilter;
use rill_types::{ChangeVersionHash, ContentRef, ObjectHash, ObjectKind, RepoPath, Timestamp};

use crate::services::common::Opened;
use rill_runtime::error::Result;

/// The objects one response lets its caller fetch.
pub struct GrantBuilder {
    filter: PathFilter,
    entries: Vec<GrantEntry>,
}

impl GrantBuilder {
    /// Entries are added only under paths `filter` allows.
    pub fn new(filter: PathFilter) -> Self {
        GrantBuilder {
            filter,
            entries: Vec::new(),
        }
    }

    fn push(&mut self, kind: ObjectKind, hash: ObjectHash, path: &RepoPath) {
        if self.filter.allows(path)
            && !self
                .entries
                .iter()
                .any(|e| e.hash == hash && e.kind == kind)
        {
            self.entries.push(GrantEntry {
                kind,
                hash,
                path: path.to_owned(),
            });
        }
    }

    /// The content of the file at `path`.
    pub fn content(&mut self, content: &ContentRef, path: &RepoPath) {
        match content {
            ContentRef::Inline { .. } => {}
            ContentRef::Blob { hash, .. } => self.push(ObjectKind::Blob, *hash, path),
            ContentRef::Chunked { hash, .. } => self.push(ObjectKind::BlobManifest, *hash, path),
        }
    }

    /// The directory at `path`. A tree object names every child, so it is
    /// granted only when its whole subtree is readable, which for
    /// inherited grants means `path` itself is.
    pub fn tree(&mut self, hash: ObjectHash, path: &RepoPath) {
        self.push(ObjectKind::Tree, hash, path);
    }

    /// A version object, which names every changed path: only for callers
    /// who may read the whole repository.
    pub fn version(&mut self, hash: ChangeVersionHash) {
        self.push(
            ObjectKind::ChangeVersion,
            hash,
            &rill_types::RepoPathBuf::root(),
        );
    }

    /// The signed grant, or an empty string when nothing was added.
    pub fn sign(self, opened: &Opened) -> Result<String> {
        if self.entries.is_empty() {
            return Ok(String::new());
        }
        let now = Timestamp::now();
        let ttl = opened
            .ctx
            .claims
            .as_ref()
            .map_or(i64::MAX, |c| c.expires_at.millis() - now.millis());
        let signer = opened.repo.identity.grant_signer()?;
        Ok(signer.sign(
            &opened.ctx.principal,
            opened.repo.id(),
            self.entries,
            now,
            ttl.max(0),
        )?)
    }
}
