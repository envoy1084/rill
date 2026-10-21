//! The two kinds of item a split list holds: directory entries, and the
//! names of a large directory in case-folded order.

use rill_types::limits::case_key;
use rill_types::{DirSummary, Tree, TreeEntry, TreeNode};

use crate::error::{Error, Result};
use crate::tree::ops::NodeRef;

/// What one split list holds and how its leaves are stored.
pub(crate) trait Layout: Send + Sync + 'static {
    type Item: Clone + Send + Sync + 'static;

    /// The sort and boundary key of an item.
    fn key(item: &Self::Item) -> &str;

    fn leaf(items: Vec<Self::Item>) -> TreeNode;

    fn items(node: &NodeRef) -> Result<Vec<Self::Item>>;

    /// The item with `key` in a leaf.
    fn find(node: &NodeRef, key: &str) -> Result<Option<Self::Item>> {
        Ok(Self::items(node)?.into_iter().find(|i| Self::key(i) == key))
    }

    fn summary(items: &[Self::Item]) -> DirSummary;
}

/// Directory entries by name.
pub(crate) struct Entries;

impl Layout for Entries {
    type Item = (String, TreeEntry);

    fn key(item: &Self::Item) -> &str {
        &item.0
    }

    fn leaf(items: Vec<Self::Item>) -> TreeNode {
        TreeNode::Leaf(Tree {
            entries: items.into_iter().collect(),
        })
    }

    fn items(node: &NodeRef) -> Result<Vec<Self::Item>> {
        match node {
            NodeRef::Leaf(tree) => Ok(tree
                .entries
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()),
            _ => Err(Error::Codec("expected a directory leaf".into())),
        }
    }

    fn find(node: &NodeRef, key: &str) -> Result<Option<Self::Item>> {
        match node {
            NodeRef::Leaf(tree) => Ok(tree.get(key).map(|e| (key.to_owned(), e.clone()))),
            _ => Err(Error::Codec("expected a directory leaf".into())),
        }
    }

    fn summary(items: &[Self::Item]) -> DirSummary {
        items
            .iter()
            .map(|(_, e)| DirSummary::of_entry(&e.kind))
            .fold(DirSummary::default(), DirSummary::merge)
    }
}

/// A large directory's names keyed `case_key(name) NUL name`, so all
/// names that collide on a case-insensitive filesystem sit together.
pub(crate) struct Names;

/// The name-index key of `name`.
pub(crate) fn name_key(name: &str) -> String {
    format!("{}\0{name}", case_key(name))
}

/// The prefix every name-index key colliding with `name` starts with.
pub(crate) fn collision_prefix(name: &str) -> String {
    format!("{}\0", case_key(name))
}

impl Layout for Names {
    type Item = String;

    fn key(item: &Self::Item) -> &str {
        item
    }

    fn leaf(items: Vec<Self::Item>) -> TreeNode {
        TreeNode::Names(items)
    }

    fn items(node: &NodeRef) -> Result<Vec<Self::Item>> {
        match node {
            NodeRef::Names(names) => Ok(names.to_vec()),
            _ => Err(Error::Codec("expected a name-index leaf".into())),
        }
    }

    fn summary(_: &[Self::Item]) -> DirSummary {
        DirSummary::default()
    }
}
