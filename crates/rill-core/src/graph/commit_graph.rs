//! Parent lookups, generation numbers, ancestry and merge bases.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;

use rill_store::RepoStore;
use rill_types::{ChangeInfo, ChangeVersionHash};

use crate::derive::{Ancestry, AncestryInfo, DeriveManager};
use crate::error::{Error, Result};
use crate::util::LruCache;

/// Versions whose parents and ancestry are kept in memory.
const GRAPH_CACHE: usize = 200_000;

/// In-process view of the change graph with cached parents and
/// generation numbers.
pub struct CommitGraph {
    repo: RepoStore,
    derive: Arc<DeriveManager>,
    parents: LruCache<ChangeVersionHash, Arc<[ChangeVersionHash]>>,
    ancestry: LruCache<ChangeVersionHash, AncestryInfo>,
}

/// Walk colors for the merge-base search.
const LEFT: u8 = 1;
const RIGHT: u8 = 2;
const STALE: u8 = 4;

impl CommitGraph {
    /// Create a graph over `repo`, using `derive` for generation numbers.
    pub fn new(repo: RepoStore, derive: Arc<DeriveManager>) -> Self {
        CommitGraph {
            repo,
            derive,
            parents: LruCache::new(GRAPH_CACHE),
            ancestry: LruCache::new(GRAPH_CACHE),
        }
    }

    /// Metadata of a version.
    pub async fn info(&self, hash: &ChangeVersionHash) -> Result<ChangeInfo> {
        let info = self
            .repo
            .get_change_info(hash)
            .await?
            .ok_or(Error::ChangeNotFound(*hash))?;
        self.parents.put(*hash, info.parents.clone().into());
        Ok(info)
    }

    /// Parents of a version, cached.
    pub async fn parents(&self, hash: &ChangeVersionHash) -> Result<Arc<[ChangeVersionHash]>> {
        if let Some(p) = self.parents.get(hash) {
            return Ok(p);
        }
        let info = self.info(hash).await?;
        Ok(info.parents.into())
    }

    /// Ancestry record of a version, cached.
    pub async fn ancestry(&self, hash: &ChangeVersionHash) -> Result<AncestryInfo> {
        if let Some(a) = self.ancestry.get(hash) {
            return Ok(a);
        }
        let a = self.derive.get::<Ancestry>(hash).await?;
        self.ancestry.put(*hash, a);
        Ok(a)
    }

    /// Generation number of a version, cached.
    pub async fn generation(&self, hash: &ChangeVersionHash) -> Result<u64> {
        Ok(self.ancestry(hash).await?.generation)
    }

    /// The first-parent ancestor of `from` at `depth`, in O(log) steps.
    pub async fn ancestor_at_depth(
        &self,
        from: &ChangeVersionHash,
        depth: u64,
    ) -> Result<Option<ChangeVersionHash>> {
        let mut hash = *from;
        let mut info = self.ancestry(&hash).await?;
        if depth > info.depth {
            return Ok(None);
        }
        while info.depth > depth {
            hash = if info.jump_depth >= depth {
                info.jump
            } else {
                info.parent.ok_or(Error::ChangeNotFound(hash))?
            };
            info = self.ancestry(&hash).await?;
        }
        Ok(Some(hash))
    }

    /// True when `ancestor` is `descendant` or one of its ancestors.
    ///
    /// Without merges behind `descendant` the history is a tree, and the
    /// answer is whether its first-parent ancestor at `ancestor`'s depth is
    /// `ancestor`. Otherwise it walks from `descendant` towards the roots,
    /// never expanding a version whose generation is at or below the
    /// target's, since nothing below it can reach the target.
    pub async fn is_ancestor(
        &self,
        ancestor: &ChangeVersionHash,
        descendant: &ChangeVersionHash,
    ) -> Result<bool> {
        if ancestor == descendant {
            return Ok(true);
        }
        let (a, d) = (
            self.ancestry(ancestor).await?,
            self.ancestry(descendant).await?,
        );
        if !d.merges {
            return Ok(self.ancestor_at_depth(descendant, a.depth).await? == Some(*ancestor));
        }
        let target_gen = a.generation;
        let mut seen = HashSet::new();
        let mut heap = BinaryHeap::new();
        heap.push((self.generation(descendant).await?, *descendant));
        while let Some((gen, hash)) = heap.pop() {
            if gen <= target_gen {
                continue;
            }
            for parent in self.parents(&hash).await?.iter() {
                if parent == ancestor {
                    return Ok(true);
                }
                if seen.insert(*parent) {
                    heap.push((self.generation(parent).await?, *parent));
                }
            }
        }
        Ok(false)
    }

    /// A best common ancestor of `a` and `b`: the first of
    /// [`merge_bases`](Self::merge_bases).
    pub async fn merge_base(
        &self,
        a: &ChangeVersionHash,
        b: &ChangeVersionHash,
    ) -> Result<Option<ChangeVersionHash>> {
        Ok(self.merge_bases(a, b).await?.into_iter().next())
    }

    /// Every best common ancestor of `a` and `b` (none is an ancestor of
    /// another), highest generation first, ties broken by hash. More than
    /// one means a criss-cross history.
    ///
    /// Two-colour walk in decreasing generation order, like git's
    /// `paint_down_to_common`: a version reached from both sides is a
    /// candidate, and its ancestors are marked stale so they are never
    /// reported. The walk ends once every queued version is stale.
    pub async fn merge_bases(
        &self,
        a: &ChangeVersionHash,
        b: &ChangeVersionHash,
    ) -> Result<Vec<ChangeVersionHash>> {
        if a == b {
            return Ok(vec![*a]);
        }
        let (ia, ib) = (self.ancestry(a).await?, self.ancestry(b).await?);
        if !ia.merges && !ib.merges {
            return Ok(self
                .common_ancestor(a, ia, b, ib)
                .await?
                .into_iter()
                .collect());
        }
        let mut flags: HashMap<ChangeVersionHash, u8> = HashMap::new();
        let mut heap: BinaryHeap<(u64, ChangeVersionHash)> = BinaryHeap::new();
        let mut found: Vec<(u64, ChangeVersionHash)> = Vec::new();
        flags.insert(*a, LEFT);
        flags.insert(*b, RIGHT);
        heap.push((self.generation(a).await?, *a));
        heap.push((self.generation(b).await?, *b));
        // The frontier is scanned for a non-stale entry before each step;
        // it is bounded by the width of the graph, not its depth.
        while heap.iter().any(|(_, h)| flags[h] & STALE == 0) {
            let Some((gen, hash)) = heap.pop() else { break };
            let mut flag = flags[&hash];
            if flag & (LEFT | RIGHT) == LEFT | RIGHT && flag & STALE == 0 {
                found.push((gen, hash));
                flag |= STALE;
                flags.insert(hash, flag);
            }
            for parent in self.parents(&hash).await?.iter() {
                let entry = flags.entry(*parent).or_insert(0);
                let before = *entry;
                *entry |= flag;
                if *entry != before {
                    heap.push((self.generation(parent).await?, *parent));
                }
            }
        }
        found.sort_by_key(|&(gen, hash)| (Reverse(gen), hash));
        found.dedup();
        Ok(found.into_iter().map(|(_, h)| h).collect())
    }

    /// The nearest common ancestor of two versions in a merge-free
    /// history. Jump pointers depend only on depth, so two versions at one
    /// depth jump to one depth, and the search takes O(log) steps.
    async fn common_ancestor(
        &self,
        a: &ChangeVersionHash,
        ia: AncestryInfo,
        b: &ChangeVersionHash,
        ib: AncestryInfo,
    ) -> Result<Option<ChangeVersionHash>> {
        let depth = ia.depth.min(ib.depth);
        let (Some(mut x), Some(mut y)) = (
            self.ancestor_at_depth(a, depth).await?,
            self.ancestor_at_depth(b, depth).await?,
        ) else {
            return Ok(None);
        };
        while x != y {
            let (ix, iy) = (self.ancestry(&x).await?, self.ancestry(&y).await?);
            match (ix.parent, iy.parent) {
                (Some(px), Some(py)) => {
                    if ix.jump != iy.jump {
                        x = ix.jump;
                        y = iy.jump;
                    } else {
                        x = px;
                        y = py;
                    }
                }
                _ => return Ok(None),
            }
        }
        Ok(Some(x))
    }
}
