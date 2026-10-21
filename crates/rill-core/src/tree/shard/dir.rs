//! Directory operations that work the same on one-node and split
//! directories.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rill_types::limits::case_key;
use rill_types::objects::tree::SHARD_ENTRIES;
use rill_types::{DirSummary, Tree, TreeEntry, TreeHash, TreeNode};

use super::layout::{collision_prefix, name_key, Entries, Names};
use super::list;
use crate::error::{Error, Result};
use crate::tree::ops::{NodeRef, TreeOps};

/// Changes to one directory, by entry name.
#[derive(Debug, Default)]
pub struct DirEdits {
    /// New entry, or `None` to remove.
    pub set: BTreeMap<String, Option<TreeEntry>>,
    /// Names that did not exist before.
    pub added: Vec<String>,
    /// Names that existed and are removed.
    pub removed: Vec<String>,
}

fn names_top(node: &NodeRef) -> Result<TreeHash> {
    match node {
        NodeRef::Inner {
            names: Some(names), ..
        } => Ok(*names),
        _ => Err(Error::Codec("a split directory has no name index".into())),
    }
}

impl TreeOps {
    /// Every entry of the directory `hash`. Loads all leaves of a split
    /// directory; prefer [`TreeOps::entry`] for single names.
    pub async fn get_tree(&self, hash: &TreeHash) -> Result<Arc<Tree>> {
        match self.get_node(hash).await? {
            NodeRef::Leaf(tree) => Ok(tree),
            NodeRef::Inner { .. } => {
                let leaves = list::leaves(self, hash).await?;
                let items = list::items::<Entries>(self, &leaves).await?;
                Ok(Arc::new(Tree {
                    entries: items.into_iter().collect(),
                }))
            }
            NodeRef::Names(_) => Err(Error::Codec("expected a directory".into())),
        }
    }

    /// One entry of the directory `dir`.
    pub async fn entry(&self, dir: &TreeHash, name: &str) -> Result<Option<TreeEntry>> {
        match self.get_node(dir).await? {
            NodeRef::Leaf(tree) => Ok(tree.get(name).cloned()),
            NodeRef::Inner { .. } => Ok(list::find::<Entries>(self, dir, name)
                .await?
                .map(|(_, entry)| entry)),
            NodeRef::Names(_) => Err(Error::Codec("expected a directory".into())),
        }
    }

    /// Summary of the directory `dir`, without loading a split
    /// directory's leaves.
    pub async fn summary(&self, dir: &TreeHash) -> Result<DirSummary> {
        Ok(match self.get_node(dir).await? {
            NodeRef::Leaf(tree) => tree.summary(),
            NodeRef::Inner { children, .. } => children
                .iter()
                .map(|c| c.summary)
                .fold(DirSummary::default(), DirSummary::merge),
            NodeRef::Names(_) => return Err(Error::Codec("expected a directory".into())),
        })
    }

    /// Number of entries of the directory `dir`.
    pub async fn dir_len(&self, dir: &TreeHash) -> Result<u64> {
        Ok(match self.get_node(dir).await? {
            NodeRef::Leaf(tree) => tree.len() as u64,
            NodeRef::Inner { children, .. } => children.iter().map(|c| c.count).sum(),
            NodeRef::Names(_) => return Err(Error::Codec("expected a directory".into())),
        })
    }

    /// Store a directory, splitting it when it is large.
    pub async fn put_tree(&self, tree: &Tree) -> Result<TreeHash> {
        if tree.len() <= SHARD_ENTRIES {
            return self.put_node(TreeNode::Leaf(tree.clone())).await;
        }
        let items: Vec<_> = tree
            .entries
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let leaves = list::write_leaves::<Entries>(self, items).await?;
        let mut names: Vec<String> = tree.entries.keys().map(|n| name_key(n)).collect();
        names.sort();
        let name_leaves = list::write_leaves::<Names>(self, names).await?;
        let names = list::write_inner(self, name_leaves, None).await?;
        list::write_inner(self, leaves, Some(names)).await
    }

    /// Apply `edits` to the directory `base` (or to an empty one).
    pub async fn edit_dir(&self, base: Option<TreeHash>, edits: DirEdits) -> Result<TreeHash> {
        let node = match base {
            Some(h) => Some(self.get_node(&h).await?),
            None => None,
        };
        let (Some(top), Some(node @ NodeRef::Inner { .. })) = (base, node.clone()) else {
            let mut tree = match node {
                Some(NodeRef::Leaf(tree)) => (*tree).clone(),
                Some(_) => return Err(Error::Codec("expected a directory".into())),
                None => Tree::empty(),
            };
            for (name, entry) in edits.set {
                match entry {
                    Some(entry) => tree.entries.insert(name, entry),
                    None => tree.entries.remove(&name),
                };
            }
            return self.put_tree(&tree).await;
        };
        let leaves = list::leaves(self, &top).await?;
        let edits_by_key = edits
            .set
            .into_iter()
            .map(|(name, entry)| (name.clone(), entry.map(|e| (name, e))))
            .collect();
        let leaves = list::edit::<Entries>(self, &leaves, edits_by_key).await?;
        let count: u64 = leaves.iter().map(|c| c.count).sum();
        if count as usize <= SHARD_ENTRIES {
            let items = list::items::<Entries>(self, &leaves).await?;
            return self
                .put_node(TreeNode::Leaf(Tree {
                    entries: items.into_iter().collect(),
                }))
                .await;
        }
        let names = names_top(&node)?;
        let mut name_edits = BTreeMap::new();
        for name in &edits.added {
            name_edits.insert(name_key(name), Some(name_key(name)));
        }
        for name in &edits.removed {
            name_edits.insert(name_key(name), None);
        }
        let names = if name_edits.is_empty() {
            names
        } else {
            let name_leaves = list::leaves(self, &names).await?;
            let name_leaves = list::edit::<Names>(self, &name_leaves, name_edits).await?;
            list::write_inner(self, name_leaves, None).await?
        };
        list::write_inner(self, leaves, Some(names)).await
    }

    /// Fail if a name in `added` would differ only in case from another
    /// entry of the directory after the edit.
    pub async fn check_case(
        &self,
        base: Option<TreeHash>,
        added: &[String],
        removed: &[String],
    ) -> Result<()> {
        if added.is_empty() {
            return Ok(());
        }
        let removed: BTreeSet<&str> = removed.iter().map(String::as_str).collect();
        let mut seen: BTreeMap<String, &str> = BTreeMap::new();
        for name in added {
            if let Some(other) = seen.insert(case_key(name), name) {
                return Err(collision(name, other));
            }
        }
        let Some(base) = base else {
            return Ok(());
        };
        let existing: Vec<String> = match self.get_node(&base).await? {
            NodeRef::Leaf(tree) => {
                let wanted: BTreeSet<String> = added.iter().map(|n| case_key(n)).collect();
                tree.entries
                    .keys()
                    .filter(|n| wanted.contains(&case_key(n)))
                    .cloned()
                    .collect()
            }
            node @ NodeRef::Inner { .. } => {
                let leaves = list::leaves(self, &names_top(&node)?).await?;
                let mut found = Vec::new();
                for name in added {
                    for key in
                        list::with_prefix::<Names>(self, &leaves, &collision_prefix(name)).await?
                    {
                        let (_, other) = key.split_once('\0').unwrap_or_default();
                        found.push(other.to_owned());
                    }
                }
                found
            }
            NodeRef::Names(_) => return Err(Error::Codec("expected a directory".into())),
        };
        for other in existing {
            if removed.contains(other.as_str()) {
                continue;
            }
            if let Some(name) = seen.get(&case_key(&other)) {
                if *name != other {
                    return Err(collision(name, &other));
                }
            }
        }
        Ok(())
    }

    /// The entries of `a` and `b` that may differ. Leaves the two split
    /// directories share hold equal entries, so they are skipped.
    pub async fn differing(
        &self,
        a: Option<TreeHash>,
        b: Option<TreeHash>,
    ) -> Result<(Arc<Tree>, Arc<Tree>)> {
        let empty = || Arc::new(Tree::empty());
        let (Some(a), Some(b)) = (a, b) else {
            let a = match a {
                Some(h) => self.get_tree(&h).await?,
                None => empty(),
            };
            let b = match b {
                Some(h) => self.get_tree(&h).await?,
                None => empty(),
            };
            return Ok((a, b));
        };
        match (self.get_node(&a).await?, self.get_node(&b).await?) {
            (NodeRef::Leaf(ta), NodeRef::Leaf(tb)) => return Ok((ta, tb)),
            (NodeRef::Inner { .. }, NodeRef::Inner { .. }) => {}
            _ => return Ok((self.get_tree(&a).await?, self.get_tree(&b).await?)),
        }
        let la = list::leaves(self, &a).await?;
        let lb = list::leaves(self, &b).await?;
        let ha: BTreeSet<TreeHash> = la.iter().map(|c| c.hash).collect();
        let hb: BTreeSet<TreeHash> = lb.iter().map(|c| c.hash).collect();
        let only_a: Vec<_> = la.into_iter().filter(|c| !hb.contains(&c.hash)).collect();
        let only_b: Vec<_> = lb.into_iter().filter(|c| !ha.contains(&c.hash)).collect();
        let tree = |items: Vec<(String, TreeEntry)>| {
            Arc::new(Tree {
                entries: items.into_iter().collect(),
            })
        };
        Ok((
            tree(list::items::<Entries>(self, &only_a).await?),
            tree(list::items::<Entries>(self, &only_b).await?),
        ))
    }
}

fn collision(name: &str, other: &str) -> Error {
    Error::InvalidChange(format!("{name:?} and {other:?} differ only in case"))
}
