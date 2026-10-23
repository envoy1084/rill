//! A mount's base tree, read through the shared tree cache: path lookups
//! walk cached trees, so they cost one fetch per directory ever visited.

use std::sync::Arc;

use rill_sdk::RepoClient;
use rill_types::{
    ContentRef, DirSummary, EntryKind, FileKind, RepoPath, RepoPathBuf, SparseProfile, Tree,
    TreeHash,
};

use super::sparse::dir_may_contain;
use crate::cache::TreeCache;
use crate::error::Result;

/// A file of the base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseEntry {
    /// The path.
    pub path: RepoPathBuf,
    /// Its content.
    pub content: ContentRef,
    /// Its kind.
    pub kind: FileKind,
}

/// The base tree of a mount.
#[derive(Clone, Debug)]
pub struct Base {
    repo: RepoClient,
    root: TreeHash,
    trees: TreeCache,
}

impl Base {
    /// The tree `root` of `repo`.
    pub fn new(repo: RepoClient, root: TreeHash, trees: TreeCache) -> Self {
        Base { repo, root, trees }
    }

    /// The root tree hash.
    pub fn root(&self) -> TreeHash {
        self.root
    }

    /// The repository it is read from.
    pub fn repo(&self) -> &RepoClient {
        &self.repo
    }

    /// The same repository at another root.
    pub fn at(&self, root: TreeHash) -> Base {
        Base::new(self.repo.clone(), root, self.trees.clone())
    }

    /// A tree by hash.
    pub async fn tree(&self, hash: TreeHash) -> Result<Arc<Tree>> {
        self.trees.get(&self.repo, hash).await
    }

    /// The entry at `path`; the root is a directory.
    pub async fn entry(&self, path: &RepoPath) -> Result<Option<EntryKind>> {
        let mut kind = EntryKind::Dir {
            hash: self.root,
            summary: DirSummary::default(),
        };
        for name in path.components() {
            let EntryKind::Dir { hash, .. } = kind else {
                return Ok(None);
            };
            let tree = self.tree(hash).await?;
            match tree.entries.get(name) {
                Some(e) => kind = e.kind.clone(),
                None => return Ok(None),
            }
        }
        Ok(Some(kind))
    }

    /// The file at `path`, if it is one.
    pub async fn file(&self, path: &RepoPath) -> Result<Option<(ContentRef, FileKind)>> {
        Ok(match self.entry(path).await? {
            Some(EntryKind::File { content, kind }) => Some((content, kind)),
            _ => None,
        })
    }

    /// The directory at `path`, if it is one.
    pub async fn dir(&self, path: &RepoPath) -> Result<Option<Arc<Tree>>> {
        match self.entry(path).await? {
            Some(EntryKind::Dir { hash, .. }) => Ok(Some(self.tree(hash).await?)),
            _ => Ok(None),
        }
    }

    /// Every file at or below `dir` inside `sparse`.
    pub async fn files(&self, dir: &RepoPath, sparse: &SparseProfile) -> Result<Vec<BaseEntry>> {
        let mut out = Vec::new();
        let start = match self.entry(dir).await? {
            Some(EntryKind::Dir { hash, .. }) => hash,
            Some(EntryKind::File { content, kind }) => {
                if sparse.contains(dir) {
                    out.push(BaseEntry {
                        path: dir.to_owned_path(),
                        content,
                        kind,
                    });
                }
                return Ok(out);
            }
            None => return Ok(out),
        };
        let mut pending = vec![(dir.to_owned_path(), start)];
        while let Some((path, hash)) = pending.pop() {
            let tree = self.tree(hash).await?;
            for (name, entry) in &tree.entries {
                let child = path.join(name)?;
                match &entry.kind {
                    EntryKind::Dir { hash, .. } => {
                        if dir_may_contain(sparse, &child) {
                            pending.push((child, *hash));
                        }
                    }
                    EntryKind::File { content, kind } => {
                        if sparse.contains(&child) {
                            out.push(BaseEntry {
                                path: child,
                                content: content.clone(),
                                kind: *kind,
                            });
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }
}
