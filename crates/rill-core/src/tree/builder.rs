//! Bottom-up tree construction from a set of path edits.
//!
//! Edits are collected into a trie keyed by path component; `build` then
//! walks the trie once, loading only the base subtrees that have an edit
//! beneath them and writing only the subtrees whose content changed.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use rill_types::{ContentRef, EntryKind, FileKind, RepoPath, Tree, TreeEntry, TreeHash};

use super::ops::{NodeRef, TreeOps};
use super::shard::DirEdits;
use crate::error::Result;

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

/// Returns the subtree hash and whether it differs from `base`. Only the
/// edited names are looked up, so a large base directory is never read
/// whole.
fn build_dir(ops: &TreeOps, base: Option<TreeHash>, node: Node) -> BuildFuture<'_> {
    Box::pin(async move {
        let mut edits = DirEdits::default();
        let base_node = match base {
            Some(h) => Some(ops.get_node(&h).await?),
            None => None,
        };
        for (name, mut child) in node.children {
            let before = match (&base_node, base) {
                (Some(NodeRef::Leaf(tree)), _) => tree.get(&name).cloned(),
                (Some(_), Some(h)) => ops.entry(&h, &name).await?,
                _ => None,
            };
            let mut current = before.clone();
            if let Some(edit) = child.edit.take() {
                current = match edit {
                    Edit::Set(content, kind) => Some(entry(EntryKind::File { content, kind })),
                    Edit::Delete => None,
                    Edit::Dir(hash) => Some(entry(EntryKind::Dir {
                        hash,
                        summary: ops.summary(&hash).await?,
                    })),
                };
            }
            if !child.children.is_empty() {
                // A file (or nothing) at this name means the nested edits
                // build a fresh directory; a directory is edited in place.
                let (child_base, had_file) = match current.as_ref().map(|e| &e.kind) {
                    Some(EntryKind::Dir { hash, .. }) => (Some(*hash), false),
                    Some(EntryKind::File { .. }) => (None, true),
                    None => (None, false),
                };
                let (hash, child_changed) = build_dir(ops, child_base, child).await?;
                if child_changed || had_file {
                    current = if ops.dir_len(&hash).await? == 0 {
                        None
                    } else {
                        Some(entry(EntryKind::Dir {
                            hash,
                            summary: ops.summary(&hash).await?,
                        }))
                    };
                }
            }
            if current == before {
                continue;
            }
            match (&before, &current) {
                (None, Some(_)) => edits.added.push(name.clone()),
                (Some(_), None) => edits.removed.push(name.clone()),
                _ => {}
            }
            edits.set.insert(name, current);
        }
        if edits.set.is_empty() {
            if let Some(base) = base {
                return Ok((base, false));
            }
        }
        ops.check_case(base, &edits.added, &edits.removed).await?;
        let hash = ops.edit_dir(base, edits).await?;
        Ok((hash, Some(hash) != base))
    })
}

fn entry(kind: EntryKind) -> TreeEntry {
    TreeEntry { kind }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::RepoPathBuf;
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

    #[tokio::test]
    async fn empty_tree_hash_matches_types() {
        assert_eq!(ops().empty_tree().await.unwrap(), Tree::empty_hash());
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
