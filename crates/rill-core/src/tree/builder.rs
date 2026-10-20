//! Bottom-up tree construction from a set of path edits.
//!
//! Edits are collected into a trie keyed by path component; `build` then
//! walks the trie once, loading only the base subtrees that have an edit
//! beneath them and writing only the subtrees whose content changed.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use rill_types::limits::case_key;
use rill_types::{ContentRef, EntryKind, FileKind, RepoPath, Tree, TreeHash};

use super::ops::TreeOps;
use crate::error::{Error, Result};

/// One edit to apply at a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// The path becomes a file with this content.
    Set(ContentRef, FileKind),
    /// The path (file or whole directory) is removed.
    Delete,
    /// The path becomes a directory with this tree.
    Dir(TreeHash),
}

#[derive(Default)]
struct Node {
    edit: Option<Edit>,
    children: BTreeMap<String, Node>,
}

/// Accumulates edits and writes the resulting tree.
#[derive(Default)]
pub struct TreeBuilder {
    root: Node,
}

impl TreeBuilder {
    /// An empty builder.
    pub fn new() -> Self {
        TreeBuilder::default()
    }

    /// Record an edit; a later edit at the same path replaces an earlier one.
    pub fn edit(&mut self, path: &RepoPath, edit: Edit) {
        let mut node = &mut self.root;
        for component in path.components() {
            node = node.children.entry(component.to_string()).or_default();
        }
        node.edit = Some(edit);
    }

    /// True when no edits were recorded.
    pub fn is_empty(&self) -> bool {
        self.root.edit.is_none() && self.root.children.is_empty()
    }

    /// Apply the edits on top of `base` and return the new root hash.
    ///
    /// An edit at the root path itself is ignored: the root is always a
    /// directory.
    pub async fn build(self, ops: &TreeOps, base: Option<TreeHash>) -> Result<TreeHash> {
        if self.root.children.is_empty() {
            return match base {
                Some(h) => Ok(h),
                None => ops.put_tree(&Tree::empty()).await,
            };
        }
        let (hash, _) = build_dir(ops, base, self.root).await?;
        Ok(hash)
    }
}

/// Boxed future of a subtree build, needed because the build recurses.
type BuildFuture<'a> = Pin<Box<dyn Future<Output = Result<(TreeHash, bool)>> + Send + 'a>>;

/// Returns the subtree hash and whether it differs from `base`.
fn build_dir(ops: &TreeOps, base: Option<TreeHash>, node: Node) -> BuildFuture<'_> {
    Box::pin(async move {
        let mut tree = match base {
            Some(h) => (*ops.get_tree(&h).await?).clone(),
            None => Tree::empty(),
        };
        let mut changed = base.is_none();
        let mut added: Vec<String> = Vec::new();
        for (name, mut child) in node.children {
            if !tree.entries.contains_key(&name) {
                added.push(name.clone());
            }
            if let Some(edit) = child.edit.take() {
                let before = tree.entries.get(&name).cloned();
                match edit {
                    Edit::Set(content, kind) => tree.insert_file(name.clone(), content, kind),
                    Edit::Delete => {
                        tree.remove(&name);
                    }
                    Edit::Dir(hash) => {
                        let summary = ops.get_tree(&hash).await?.summary();
                        tree.insert_dir(name.clone(), hash, summary);
                    }
                }
                changed |= tree.entries.get(&name) != before.as_ref();
            }
            if child.children.is_empty() {
                continue;
            }
            // A file (or nothing) at this name means the nested edits build
            // a fresh directory; an existing directory is edited in place.
            let child_base = match tree.entries.get(&name).map(|e| &e.kind) {
                Some(EntryKind::Dir { hash, .. }) => Some(*hash),
                _ => None,
            };
            let had_file = matches!(
                tree.entries.get(&name).map(|e| &e.kind),
                Some(EntryKind::File { .. })
            );
            let (hash, child_changed) = build_dir(ops, child_base, child).await?;
            if !child_changed && !had_file {
                continue;
            }
            changed = true;
            let subtree = ops.get_tree(&hash).await?;
            if subtree.is_empty() {
                tree.remove(&name);
            } else {
                tree.insert_dir(name, hash, subtree.summary());
            }
        }
        check_case_collisions(&tree, &added)?;
        if !changed {
            // Cannot be None: `changed` starts true when there is no base.
            return Ok((base.expect("unchanged tree has a base"), false));
        }
        let hash = ops.put_tree(&tree).await?;
        Ok((hash, Some(hash) != base))
    })
}

/// A name this build added must not collide by case with another entry,
/// or the directory cannot be checked out on case-insensitive filesystems.
/// Entries that were already there are left alone.
fn check_case_collisions(tree: &Tree, added: &[String]) -> Result<()> {
    for name in added.iter().filter(|n| tree.entries.contains_key(*n)) {
        let key = case_key(name);
        if let Some(other) = tree
            .entries
            .keys()
            .find(|other| *other != name && case_key(other) == key)
        {
            return Err(Error::InvalidChange(format!(
                "{name:?} and {other:?} differ only in case"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{Encodable, RepoPathBuf};
    use std::sync::Arc;

    fn ops() -> TreeOps {
        TreeOps::new(Arc::new(rill_store::MemoryObjectStore::new()), 64)
    }

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    fn set(bytes: &[u8]) -> Edit {
        Edit::Set(ContentRef::inline(bytes.to_vec()), FileKind::Regular)
    }

    #[tokio::test]
    async fn builds_and_edits_nested_tree() {
        let ops = ops();
        let mut b = TreeBuilder::new();
        b.edit(&p("a/b/c.txt"), set(b"c"));
        b.edit(&p("a/d.txt"), set(b"d"));
        b.edit(&p("top.txt"), set(b"t"));
        let root = b.build(&ops, None).await.unwrap();
        assert!(matches!(
            ops.lookup(root, &p("a/b/c.txt")).await.unwrap(),
            Some(EntryKind::File { .. })
        ));

        // Deleting the only file in a/b removes a/b itself.
        let mut b = TreeBuilder::new();
        b.edit(&p("a/b/c.txt"), Edit::Delete);
        let root2 = b.build(&ops, Some(root)).await.unwrap();
        assert!(ops.lookup(root2, &p("a/b")).await.unwrap().is_none());
        assert!(ops.lookup(root2, &p("a/d.txt")).await.unwrap().is_some());

        // No-op edits leave the hash untouched.
        let mut b = TreeBuilder::new();
        b.edit(&p("a/d.txt"), set(b"d"));
        assert_eq!(b.build(&ops, Some(root2)).await.unwrap(), root2);
    }

    #[test]
    fn empty_tree_hash_matches_types() {
        assert_eq!(Tree::empty().hash(), Tree::default().hash());
    }

    #[tokio::test]
    async fn file_replaced_by_directory() {
        let ops = ops();
        let mut b = TreeBuilder::new();
        b.edit(&p("x"), set(b"file"));
        let root = b.build(&ops, None).await.unwrap();
        let mut b = TreeBuilder::new();
        b.edit(&p("x"), Edit::Delete);
        b.edit(&p("x/y"), set(b"inner"));
        let root2 = b.build(&ops, Some(root)).await.unwrap();
        assert!(matches!(
            ops.lookup(root2, &p("x")).await.unwrap(),
            Some(EntryKind::Dir { .. })
        ));
        assert!(ops.lookup(root2, &p("x/y")).await.unwrap().is_some());

        // And back: a directory replaced by a file.
        let mut b = TreeBuilder::new();
        b.edit(&p("x"), set(b"file again"));
        let root3 = b.build(&ops, Some(root2)).await.unwrap();
        assert!(matches!(
            ops.lookup(root3, &p("x")).await.unwrap(),
            Some(EntryKind::File { .. })
        ));
    }
}
