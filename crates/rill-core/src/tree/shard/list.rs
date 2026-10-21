//! A sorted list stored as leaves under inner nodes.
//!
//! Leaf boundaries come from [`shape`](rill_types::objects::tree::shape),
//! so the nodes are a function of the items alone. Editing re-chunks only
//! from the first edited leaf until the new chunking lands on an old leaf
//! boundary again; every other leaf is reused as is. Inner levels are
//! rebuilt from the leaf list, which is small, and only the inner nodes
//! that changed are written.

use std::collections::BTreeMap;

use rill_types::objects::tree::shape::{chunk, Chunker};
use rill_types::{TreeChild, TreeHash, TreeNode};

use super::layout::Layout;
use crate::error::{Error, Result};
use crate::tree::ops::{NodeRef, TreeOps};

/// The leaf pointers of the list under `top`, in order.
pub(crate) async fn leaves(ops: &TreeOps, top: &TreeHash) -> Result<Vec<TreeChild>> {
    let mut level_nodes = vec![*top];
    loop {
        let mut next = Vec::new();
        let mut reached_leaves = false;
        for hash in &level_nodes {
            match ops.get_node(hash).await? {
                NodeRef::Inner {
                    level, children, ..
                } => {
                    reached_leaves = level == 1;
                    next.extend(children.iter().cloned());
                }
                _ => return Err(Error::Codec("expected an inner tree node".into())),
            }
        }
        if reached_leaves {
            return Ok(next);
        }
        level_nodes = next.into_iter().map(|c| c.hash).collect();
    }
}

/// Every item of the list, in order.
pub(crate) async fn items<L: Layout>(ops: &TreeOps, leaves: &[TreeChild]) -> Result<Vec<L::Item>> {
    let mut out = Vec::new();
    for leaf in leaves {
        out.extend(L::items(&ops.get_node(&leaf.hash).await?)?);
    }
    Ok(out)
}

/// The item with `key`, found by descending from `top`.
pub(crate) async fn find<L: Layout>(
    ops: &TreeOps,
    top: &TreeHash,
    key: &str,
) -> Result<Option<L::Item>> {
    let mut hash = *top;
    loop {
        let node = ops.get_node(&hash).await?;
        match &node {
            NodeRef::Inner { children, .. } => {
                let i = children.partition_point(|c| c.first.as_str() <= key);
                if i == 0 {
                    return Ok(None);
                }
                hash = children[i - 1].hash;
            }
            _ => return L::find(&node, key),
        }
    }
}

/// The items whose keys start with `prefix`, in order.
pub(crate) async fn with_prefix<L: Layout>(
    ops: &TreeOps,
    leaves: &[TreeChild],
    prefix: &str,
) -> Result<Vec<L::Item>> {
    let start = leaves
        .partition_point(|c| c.first.as_str() < prefix)
        .saturating_sub(1);
    let mut out = Vec::new();
    for leaf in &leaves[start..] {
        if leaf.first.as_str() > prefix && !leaf.first.starts_with(prefix) {
            break;
        }
        for item in L::items(&ops.get_node(&leaf.hash).await?)? {
            if L::key(&item).starts_with(prefix) {
                out.push(item);
            }
        }
    }
    Ok(out)
}

async fn write_leaf<L: Layout>(ops: &TreeOps, items: Vec<L::Item>) -> Result<TreeChild> {
    let first = L::key(&items[0]).to_owned();
    let count = items.len() as u64;
    let summary = L::summary(&items);
    let hash = ops.put_node(L::leaf(items)).await?;
    Ok(TreeChild {
        first,
        hash,
        count,
        summary,
    })
}

/// Chunk sorted `items` into leaves and write them.
pub(crate) async fn write_leaves<L: Layout>(
    ops: &TreeOps,
    items: Vec<L::Item>,
) -> Result<Vec<TreeChild>> {
    let mut out = Vec::new();
    for node in chunk(items, 0, |i| L::key(i)) {
        out.push(write_leaf::<L>(ops, node).await?);
    }
    Ok(out)
}

/// Write the inner levels above `leaves` and return the top node, which
/// carries `names`.
pub(crate) async fn write_inner(
    ops: &TreeOps,
    leaves: Vec<TreeChild>,
    names: Option<TreeHash>,
) -> Result<TreeHash> {
    let mut children = leaves;
    let mut level = 1u8;
    loop {
        let nodes = chunk(children, level, |c| c.first.as_str());
        let top = nodes.len() == 1;
        let mut parents = Vec::with_capacity(nodes.len());
        for node in nodes {
            let first = node[0].first.clone();
            let count = node.iter().map(|c| c.count).sum();
            let summary = node
                .iter()
                .map(|c| c.summary)
                .fold(Default::default(), rill_types::DirSummary::merge);
            let hash = ops
                .put_node(TreeNode::Inner {
                    level,
                    children: node,
                    names: if top { names } else { None },
                })
                .await?;
            if top {
                return Ok(hash);
            }
            parents.push(TreeChild {
                first,
                hash,
                count,
                summary,
            });
        }
        children = parents;
        level = level
            .checked_add(1)
            .ok_or_else(|| Error::InvalidChange("tree too deep".into()))?;
    }
}

/// One edit: a key and its new item, or `None` to remove it.
type Edit<L> = (String, Option<<L as Layout>::Item>);

/// Apply `edits` (key to new item, `None` to remove) to the list whose
/// leaves are `leaves`, returning the new leaves.
pub(crate) async fn edit<L: Layout>(
    ops: &TreeOps,
    leaves: &[TreeChild],
    edits: BTreeMap<String, Option<L::Item>>,
) -> Result<Vec<TreeChild>> {
    let mut by_leaf: BTreeMap<usize, Vec<Edit<L>>> = BTreeMap::new();
    for (key, item) in edits {
        let i = leaves
            .partition_point(|c| c.first.as_str() <= key.as_str())
            .saturating_sub(1);
        by_leaf.entry(i).or_default().push((key, item));
    }
    let mut out = Vec::with_capacity(leaves.len() + 1);
    let mut j = 0;
    while j < leaves.len() {
        if !by_leaf.contains_key(&j) {
            out.push(leaves[j].clone());
            j += 1;
            continue;
        }
        let mut chunker = Chunker::new(0);
        loop {
            let mut merged: BTreeMap<String, L::Item> =
                L::items(&ops.get_node(&leaves[j].hash).await?)?
                    .into_iter()
                    .map(|item| (L::key(&item).to_owned(), item))
                    .collect();
            for (key, item) in by_leaf.remove(&j).unwrap_or_default() {
                match item {
                    Some(item) => merged.insert(key, item),
                    None => merged.remove(&key),
                };
            }
            for (key, item) in merged {
                if let Some(node) = chunker.push(item, &key) {
                    out.push(write_leaf::<L>(ops, node).await?);
                }
            }
            j += 1;
            // Back on an old boundary with the next leaf untouched: the
            // rest of the old chunking stands.
            if j == leaves.len() || (chunker.at_boundary() && !by_leaf.contains_key(&j)) {
                break;
            }
        }
        if let Some(node) = chunker.finish() {
            out.push(write_leaf::<L>(ops, node).await?);
        }
    }
    Ok(out)
}
