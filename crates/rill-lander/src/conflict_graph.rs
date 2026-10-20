//! Partitioning in-flight requests by the paths they touch.
//!
//! Requests whose path sets are disjoint cannot conflict with each other
//! and may be processed concurrently; requests that overlap are kept in
//! one group and processed in arrival order, so the later one is rebased
//! on the earlier one's result.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use rill_core::{diff_trees, DeriveManager, RootTree};
use rill_types::{ChangeVersionHash, RepoPathBuf};

use crate::error::Result;
use crate::queue::QueuedRequest;

/// Paths whose state `version` changes relative to its first parent,
/// plus any paths it records as conflicted.
pub async fn touched_paths(
    derive: &Arc<DeriveManager>,
    version: &ChangeVersionHash,
) -> Result<BTreeSet<RepoPathBuf>> {
    let cv = derive.change(version).await?;
    let parent_root = match cv.first_parent() {
        Some(parent) => Some(derive.get::<RootTree>(&parent).await?),
        None => None,
    };
    let root = derive.get::<RootTree>(version).await?;
    let mut paths: BTreeSet<RepoPathBuf> = diff_trees(derive.trees(), parent_root, Some(root))
        .await?
        .into_iter()
        .map(|e| e.path)
        .collect();
    paths.extend(cv.conflicted_paths().cloned());
    paths.extend(cv.subtree_changes.keys().cloned());
    Ok(paths)
}

/// Group `requests` into independent batches (see the module docs).
/// Batches are ordered by their earliest request; requests within a
/// batch keep arrival order.
pub async fn independent_batches(
    derive: &Arc<DeriveManager>,
    requests: Vec<QueuedRequest>,
) -> Result<Vec<Vec<QueuedRequest>>> {
    let mut items = Vec::with_capacity(requests.len());
    for request in requests {
        let paths = touched_paths(derive, &request.request.version).await?;
        items.push((request, paths));
    }
    Ok(partition(items))
}

/// Pure partition of `(item, touched paths)` pairs into groups whose
/// path sets are connected by overlap. Input order is arrival order.
///
/// Two paths overlap when equal or when one is a prefix directory of the
/// other, which covers a file replaced by a directory.
pub fn partition<T>(items: Vec<(T, BTreeSet<RepoPathBuf>)>) -> Vec<Vec<T>> {
    let n = items.len();
    let mut uf = UnionFind::new(n);
    // First request that touched each exact path, and first request that
    // touched something beneath each directory.
    let mut owner: HashMap<RepoPathBuf, usize> = HashMap::new();
    let mut dir_owner: HashMap<RepoPathBuf, usize> = HashMap::new();
    for (index, (_, paths)) in items.iter().enumerate() {
        for path in paths {
            if let Some(&other) = owner.get(path) {
                uf.union(index, other);
            }
            if let Some(&other) = dir_owner.get(path) {
                uf.union(index, other);
            }
            let mut ancestor = path.parent();
            while !ancestor.is_root() {
                if let Some(&other) = owner.get(ancestor) {
                    uf.union(index, other);
                }
                ancestor = ancestor.parent();
            }
        }
        for path in paths {
            owner.entry(path.clone()).or_insert(index);
            let mut ancestor = path.parent();
            while !ancestor.is_root() {
                dir_owner.entry(ancestor.to_owned()).or_insert(index);
                ancestor = ancestor.parent();
            }
        }
    }

    let mut groups: Vec<Vec<T>> = Vec::new();
    let mut group_of_root: HashMap<usize, usize> = HashMap::new();
    for (index, (item, _)) in items.into_iter().enumerate() {
        let root = uf.find(index);
        let group = *group_of_root.entry(root).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[group].push(item);
    }
    groups
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            // Attach the later root under the earlier so group order
            // follows arrival order.
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent[hi] = lo;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(paths: &[&str]) -> BTreeSet<RepoPathBuf> {
        paths.iter().map(|p| RepoPathBuf::new(p).unwrap()).collect()
    }

    #[test]
    fn disjoint_requests_are_separate_groups() {
        let groups = partition(vec![
            ("a", set(&["x/1"])),
            ("b", set(&["y/1"])),
            ("c", set(&["x/1", "z"])),
            ("d", set(&["z"])),
            ("e", set(&["q"])),
        ]);
        assert_eq!(groups, vec![vec!["a", "c", "d"], vec!["b"], vec!["e"]]);
    }

    #[test]
    fn directory_prefix_overlaps_file() {
        let groups = partition(vec![("a", set(&["dir/file"])), ("b", set(&["dir"]))]);
        assert_eq!(groups, vec![vec!["a", "b"]]);
    }

    #[test]
    fn empty_sets_never_overlap() {
        let groups = partition(vec![("a", set(&[])), ("b", set(&[]))]);
        assert_eq!(groups.len(), 2);
    }
}
