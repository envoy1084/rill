//! Fetching a server-side tree into memory: the recursive listing and the
//! file fetch used by create and update.

use std::collections::BTreeMap;

use bytes::Bytes;
use rill_types::{ContentRef, EntryKind, FileKind, RepoPathBuf, SparseProfile};

use crate::error::Result;
use crate::repo::RepoClient;
use crate::types::Root;

/// A file as the server describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    /// Content reference.
    pub content: ContentRef,
    /// File kind.
    pub kind: FileKind,
}

/// Path → file for every file under `root` inside the sparse profile.
///
/// Directories are listed one RPC at a time; a directory outside the
/// profile is never entered.
pub async fn list_tree(
    repo: &RepoClient,
    root: &Root,
    sparse: &SparseProfile,
) -> Result<BTreeMap<RepoPathBuf, RemoteFile>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![RepoPathBuf::root()];
    while let Some(dir) = pending.pop() {
        for (name, kind) in repo.list_dir(root, &dir).await? {
            let path = dir.join(&name)?;
            match kind {
                EntryKind::Dir { .. } => {
                    if dir_may_contain(sparse, &path) {
                        pending.push(path);
                    }
                }
                EntryKind::File { content, kind } => {
                    if sparse.contains(&path) {
                        files.insert(path, RemoteFile { content, kind });
                    }
                }
            }
        }
    }
    Ok(files)
}

/// True when some included path may lie under `dir`: either the dir is
/// itself included, or it is an ancestor of an include.
fn dir_may_contain(sparse: &SparseProfile, dir: &RepoPathBuf) -> bool {
    if sparse.exclude.iter().any(|e| dir.starts_with(e)) {
        return false;
    }
    sparse.include.is_empty()
        || sparse
            .include
            .iter()
            .any(|i| dir.starts_with(i) || i.starts_with(dir))
}

/// Fetch a file's bytes.
pub async fn fetch(repo: &RepoClient, file: &RemoteFile) -> Result<Bytes> {
    repo.get_blob(&file.content).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestors_of_includes_are_entered() {
        let sparse = SparseProfile {
            include: vec![RepoPathBuf::new("a/b/c").unwrap()],
            exclude: vec![RepoPathBuf::new("a/b/c/d").unwrap()],
        };
        assert!(dir_may_contain(&sparse, &RepoPathBuf::new("a").unwrap()));
        assert!(dir_may_contain(
            &sparse,
            &RepoPathBuf::new("a/b/c/e").unwrap()
        ));
        assert!(!dir_may_contain(&sparse, &RepoPathBuf::new("x").unwrap()));
        assert!(!dir_may_contain(
            &sparse,
            &RepoPathBuf::new("a/b/c/d").unwrap()
        ));
    }
}
