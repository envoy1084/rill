//! The write check every stored version passes, inside its commit, and
//! again when it lands: its writer may write every path it changes, read
//! every path it copies from, and own the repository's configuration to
//! change that.

use rill_core::{DeriveManager, TreeOps};
use rill_policy::{Relation, Verb};
use rill_types::{
    ChangeVersion, EntryKind, FileChange, RepoPath, RepoPathBuf, SubtreeChange, TreeHash,
};

use crate::auth::Permit;
use crate::error::Result;
use crate::services::common::Opened;

/// Paths under this directory configure the repository (owners, landing
/// domains, verification); changing them takes owner.
pub const CONFIG_DIR: &str = ".rill";

/// What a version writes and what it reads to do so.
#[derive(Debug, Default)]
pub struct WriteSet {
    pub writes: Vec<RepoPathBuf>,
    pub copies_from: Vec<RepoPathBuf>,
}

/// Whether changing `path` takes owner rather than writer.
pub fn is_config(path: &RepoPath) -> bool {
    path.components()
        .next()
        .is_some_and(|first| first == CONFIG_DIR)
}

/// The paths `cv` writes. A merge's changes relative to its first parent
/// include everything the other side changed; paths it leaves as another
/// parent has them are brought in, not written.
pub async fn write_set(
    trees: &TreeOps,
    derive: &std::sync::Arc<DeriveManager>,
    cv: &ChangeVersion,
) -> Result<WriteSet> {
    let mut others = Vec::new();
    for parent in cv.parents.iter().skip(1) {
        others.push(derive.get::<rill_core::RootTree>(parent).await?);
    }
    let mut set = WriteSet::default();
    for (path, change) in &cv.file_changes {
        if taken_from_other_parent(trees, &others, path, change).await? {
            continue;
        }
        set.writes.push(path.clone());
        if let FileChange::Set {
            copy_from: Some(source),
            ..
        } = change
        {
            set.copies_from.push(source.path.clone());
        }
    }
    for (path, change) in &cv.subtree_changes {
        set.writes.push(path.clone());
        if let SubtreeChange::Copy { from_path, .. } = change {
            set.copies_from.push(from_path.clone());
        }
    }
    Ok(set)
}

/// Authorize `cv` for its committer. Returns the relation version the
/// decision rests on; the commit asserts it, so a relation change racing
/// the commit forces it to be decided again.
pub async fn version(opened: &Opened, cv: &ChangeVersion) -> Result<i64> {
    let permit = opened.permit();
    let set = write_set(&opened.repo.trees, &opened.repo.derive, cv).await?;
    for path in &set.writes {
        write(&permit, path).await?;
    }
    for path in &set.copies_from {
        permit.read(path).await?;
    }
    if set.writes.is_empty() {
        permit.write_any().await?;
    }
    permit.relations_version().await
}

async fn write(permit: &Permit<'_>, path: &RepoPath) -> Result<()> {
    if is_config(path) {
        permit.check(Verb::Write, Relation::Owner, path).await
    } else {
        permit.write(path).await
    }
}

async fn taken_from_other_parent(
    trees: &TreeOps,
    others: &[TreeHash],
    path: &RepoPath,
    change: &FileChange,
) -> Result<bool> {
    for root in others {
        let theirs = trees.lookup(*root, path).await?;
        let same = match (change, theirs) {
            (FileChange::Delete, None) => true,
            (
                FileChange::Set {
                    content,
                    kind,
                    copy_from: None,
                },
                Some(EntryKind::File {
                    content: c,
                    kind: k,
                }),
            ) => *content == c && *kind == k,
            _ => false,
        };
        if same {
            return Ok(true);
        }
    }
    Ok(false)
}
