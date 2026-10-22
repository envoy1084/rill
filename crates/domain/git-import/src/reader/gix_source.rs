//! [`GitSource`] over a local repository via `gix`.

use std::path::Path;

use bytes::Bytes;
use gix::bstr::ByteSlice;
use gix::diff::tree::recorder::{Change, Location};
use gix::diff::tree::{Recorder, State};
use gix::objs::tree::EntryMode;
use gix::objs::TreeRefIter;
use gix::ObjectId;
use rill_types::{FileKind, Timestamp};

use super::source::{CommitChanges, GitChange, GitCommit, GitSha, GitSource};
use crate::error::{Error, Result};

/// A git repository on disk. No network, no worktree: only the object
/// database and refs are read.
///
/// Holds the thread-safe handle because `gix::Repository` is `!Send` and
/// the importer drives sources from a blocking thread; each call opens a
/// cheap thread-local view.
pub struct GixSource {
    repo: gix::ThreadSafeRepository,
}

impl GixSource {
    /// Open the repository at `path` (a worktree or a bare `.git`).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let repo = gix::ThreadSafeRepository::open(path.as_ref()).map_err(Error::git)?;
        Ok(GixSource { repo })
    }

    fn oid(sha: &GitSha) -> Result<ObjectId> {
        ObjectId::from_hex(sha.as_str().as_bytes()).map_err(|_| Error::CommitNotFound(sha.clone()))
    }

    fn tree_data(repo: &gix::Repository, commit: &gix::Commit<'_>) -> Result<Vec<u8>> {
        let tree_id = commit.tree_id().map_err(Error::git)?;
        Ok(repo.find_tree(tree_id).map_err(Error::git)?.data.clone())
    }

    fn convert(repo: &gix::Repository, change: Change, out: &mut CommitChanges) -> Result<()> {
        // The recorder reports directories too (with their blobs listed
        // separately); only blob-level entries matter here.
        match change {
            Change::Deletion { entry_mode, .. }
            | Change::Addition { entry_mode, .. }
            | Change::Modification { entry_mode, .. }
                if entry_mode.is_tree() =>
            {
                return Ok(());
            }
            Change::Deletion { path, .. } => out.changes.push(GitChange::Delete {
                path: path.to_str_lossy().into_owned(),
            }),
            Change::Addition {
                entry_mode,
                oid,
                path,
                ..
            }
            | Change::Modification {
                entry_mode,
                oid,
                path,
                ..
            } => {
                let path = path.to_str_lossy().into_owned();
                let Some(kind) = file_kind(entry_mode) else {
                    tracing::warn!(path, "skipping submodule entry");
                    out.warnings.push(format!("skipped submodule at {path}"));
                    return Ok(());
                };
                let blob = repo.find_blob(oid).map_err(Error::git)?;
                out.changes.push(GitChange::Set {
                    path,
                    content: Bytes::copy_from_slice(&blob.data),
                    kind,
                });
            }
        }
        Ok(())
    }
}

/// Rill file kind of a git entry mode; `None` for submodules (gitlinks).
fn file_kind(mode: EntryMode) -> Option<FileKind> {
    if mode.is_commit() {
        None
    } else if mode.is_link() {
        Some(FileKind::Symlink)
    } else if mode.is_executable() {
        Some(FileKind::Executable)
    } else {
        Some(FileKind::Regular)
    }
}

impl GitSource for GixSource {
    fn resolve(&mut self, head_ref: &str) -> Result<GitSha> {
        let repo = self.repo.to_thread_local();
        if let Ok(mut reference) = repo.find_reference(head_ref) {
            let id = reference.peel_to_id().map_err(Error::git)?;
            return Ok(GitSha::new(id.to_hex().to_string()));
        }
        let id = ObjectId::from_hex(head_ref.as_bytes())
            .map_err(|_| Error::UnknownRef(head_ref.to_string()))?;
        repo.find_commit(id)
            .map_err(|_| Error::UnknownRef(head_ref.to_string()))?;
        Ok(GitSha::new(id.to_hex().to_string()))
    }

    fn commit(&mut self, sha: &GitSha) -> Result<GitCommit> {
        let id = Self::oid(sha)?;
        let repo = self.repo.to_thread_local();
        let commit = repo
            .find_commit(id)
            .map_err(|_| Error::CommitNotFound(sha.clone()))?;
        let decoded = commit.decode().map_err(Error::git)?;
        let author = decoded.author().map_err(Error::git)?;
        let committer = decoded.committer().map_err(Error::git)?;
        let committed_at = Timestamp::from_unix_seconds(committer.seconds());
        Ok(GitCommit {
            sha: sha.clone(),
            parents: commit
                .parent_ids()
                .map(|p| GitSha::new(p.to_hex().to_string()))
                .collect(),
            author_name: author.name.to_str_lossy().into_owned(),
            author_email: author.email.to_str_lossy().into_owned(),
            committed_at,
            message: decoded.message.to_str_lossy().into_owned(),
        })
    }

    fn changes(&mut self, sha: &GitSha) -> Result<CommitChanges> {
        let id = Self::oid(sha)?;
        let repo = self.repo.to_thread_local();
        let commit = repo
            .find_commit(id)
            .map_err(|_| Error::CommitNotFound(sha.clone()))?;
        let new_tree = Self::tree_data(&repo, &commit)?;
        let old_tree = match commit.parent_ids().next() {
            Some(parent) => {
                let parent = repo.find_commit(parent).map_err(Error::git)?;
                Self::tree_data(&repo, &parent)?
            }
            None => Vec::new(),
        };
        let mut recorder = Recorder::default().track_location(Some(Location::Path));
        gix::diff::tree(
            TreeRefIter::from_bytes(&old_tree, repo.object_hash()),
            TreeRefIter::from_bytes(&new_tree, repo.object_hash()),
            State::default(),
            &repo.objects,
            &mut recorder,
        )
        .map_err(Error::git)?;
        let mut out = CommitChanges::default();
        for change in recorder.records {
            Self::convert(&repo, change, &mut out)?;
        }
        Ok(out)
    }
}
