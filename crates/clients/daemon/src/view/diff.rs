//! The files that differ between two base trees, skipping every subtree
//! whose hash is unchanged, so moving a base costs what changed upstream.

use std::collections::BTreeSet;

use rill_types::{ContentRef, EntryKind, FileKind, RepoPathBuf, SparseProfile, TreeHash};

use super::base::Base;
use super::sparse::dir_may_contain;
use crate::error::Result;

/// A file's state in one tree.
pub type FileState = Option<(ContentRef, FileKind)>;

/// One file that differs between the trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeChange {
    /// The path.
    pub path: RepoPathBuf,
    /// In the old tree.
    pub old: FileState,
    /// In the new tree.
    pub new: FileState,
}

/// Files that differ between `old` and `new` (both of `base`'s
/// repository) inside `sparse`, in path order.
pub async fn diff_trees(
    base: &Base,
    old: TreeHash,
    new: TreeHash,
    sparse: &SparseProfile,
) -> Result<Vec<TreeChange>> {
    let mut out = Vec::new();
    let mut pending = vec![(RepoPathBuf::root(), Some(old), Some(new))];
    while let Some((dir, old, new)) = pending.pop() {
        if old == new {
            continue;
        }
        let old_tree = match old {
            Some(h) => Some(base.tree(h).await?),
            None => None,
        };
        let new_tree = match new {
            Some(h) => Some(base.tree(h).await?),
            None => None,
        };
        let names: BTreeSet<&String> = old_tree
            .iter()
            .chain(new_tree.iter())
            .flat_map(|t| t.entries.keys())
            .collect();
        for name in names {
            let path = dir.join(name)?;
            let split = |tree: &Option<std::sync::Arc<rill_types::Tree>>| match tree
                .as_ref()
                .and_then(|t| t.entries.get(name))
                .map(|e| &e.kind)
            {
                Some(EntryKind::Dir { hash, .. }) => (Some(*hash), None),
                Some(EntryKind::File { content, kind }) => (None, Some((content.clone(), *kind))),
                None => (None, None),
            };
            let (old_dir, old_file) = split(&old_tree);
            let (new_dir, new_file) = split(&new_tree);
            if (old_dir.is_some() || new_dir.is_some()) && dir_may_contain(sparse, &path) {
                pending.push((path.clone(), old_dir, new_dir));
            }
            if old_file != new_file && sparse.contains(&path) {
                out.push(TreeChange {
                    path,
                    old: old_file,
                    new: new_file,
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}
