//! How directories are stored.

use serde::{Deserialize, Serialize};

use super::listing::{DirSummary, Tree, TreeHash};
use crate::encoding::{Encodable, Schema};
use crate::hash::ObjectKind;

/// A pointer from an inner node to the node below it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TreeChild {
    /// The first key in the child's subtree.
    pub first: String,
    pub hash: TreeHash,
    /// Leaf items in the child's subtree.
    pub count: u64,
    /// Summary of the directory entries in the child's subtree; empty for
    /// name-index nodes.
    pub summary: DirSummary,
}

/// One stored tree object.
///
/// A directory of at most [`SHARD_ENTRIES`](super::SHARD_ENTRIES) entries
/// is one `Leaf`. A larger directory is `Leaf` nodes under `Inner` nodes;
/// its top node carries `names`, the root of an index of its names in
/// case-folded order, so case collisions can be found without a scan.
/// The index is `Names` leaves under `Inner` nodes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TreeNode {
    Leaf(Tree),
    Inner {
        /// 1 when the children are leaves.
        level: u8,
        children: Vec<TreeChild>,
        names: Option<TreeHash>,
    },
    Names(Vec<String>),
}

impl TreeNode {
    /// Summary of the directory entries under this node.
    pub fn summary(&self) -> DirSummary {
        match self {
            TreeNode::Leaf(tree) => tree.summary(),
            TreeNode::Inner { children, .. } => children
                .iter()
                .map(|c| c.summary)
                .fold(DirSummary::default(), DirSummary::merge),
            TreeNode::Names(_) => DirSummary::default(),
        }
    }

    /// Leaf items under this node.
    pub fn count(&self) -> u64 {
        match self {
            TreeNode::Leaf(tree) => tree.len() as u64,
            TreeNode::Inner { children, .. } => children.iter().map(|c| c.count).sum(),
            TreeNode::Names(names) => names.len() as u64,
        }
    }
}

impl Encodable for TreeNode {
    const KIND: ObjectKind = ObjectKind::Tree;
    const SCHEMA: Schema = 1;
    const NAME: &'static str = "TreeNode";
}
