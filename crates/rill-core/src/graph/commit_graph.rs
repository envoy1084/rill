//! Parent lookups, generation numbers, ancestry and merge bases.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use rill_store::RepoStore;
use rill_types::{ChangeInfo, ChangeVersionHash};

use crate::derive::{DeriveManager, Generation};
use crate::error::{Error, Result};

/// In-process view of the change graph with cached parents and
/// generation numbers.
pub struct CommitGraph {
    repo: RepoStore,
    derive: Arc<DeriveManager>,
    parents: DashMap<ChangeVersionHash, Arc<[ChangeVersionHash]>>,
    generations: DashMap<ChangeVersionHash, u64>,
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
            parents: DashMap::new(),
            generations: DashMap::new(),
        }
    }

    /// Metadata of a version.
    pub async fn info(&self, hash: &ChangeVersionHash) -> Result<ChangeInfo> {
        let info = self
            .repo
            .get_change_info(hash)
            .await?
            .ok_or(Error::ChangeNotFound(*hash))?;
        self.parents.insert(*hash, info.parents.clone().into());
        Ok(info)
    }

    /// Parents of a version, cached.
    pub async fn parents(&self, hash: &ChangeVersionHash) -> Result<Arc<[ChangeVersionHash]>> {
        if let Some(p) = self.parents.get(hash) {
            return Ok(p.clone());
        }
        let info = self.info(hash).await?;
        Ok(info.parents.into())
    }

    /// Generation number of a version, cached.
    pub async fn generation(&self, hash: &ChangeVersionHash) -> Result<u64> {
        if let Some(g) = self.generations.get(hash) {
            return Ok(*g);
        }
        let g = self.derive.get::<Generation>(hash).await?;
        self.generations.insert(*hash, g);
        Ok(g)
    }

    /// True when `ancestor` is `descendant` or one of its ancestors.
    ///
    /// Walks from `descendant` towards the roots, never expanding a
    /// version whose generation is at or below the target's, since nothing
    /// below it can reach the target.
    pub async fn is_ancestor(
        &self,
        ancestor: &ChangeVersionHash,
        descendant: &ChangeVersionHash,
    ) -> Result<bool> {
        if ancestor == descendant {
            return Ok(true);
        }
        let target_gen = self.generation(ancestor).await?;
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

    /// A best common ancestor of `a` and `b`: the one with the highest
    /// generation, ties broken by hash for determinism.
    ///
    /// Two-colour walk in decreasing generation order, like git's
    /// `paint_down_to_common`: a version reached from both sides is a
    /// candidate, and its ancestors are marked stale so they are never
    /// reported. The walk ends once every queued version is stale.
    pub async fn merge_base(
        &self,
        a: &ChangeVersionHash,
        b: &ChangeVersionHash,
    ) -> Result<Option<ChangeVersionHash>> {
        if a == b {
            return Ok(Some(*a));
        }
        let mut flags: HashMap<ChangeVersionHash, u8> = HashMap::new();
        let mut heap: BinaryHeap<(u64, ChangeVersionHash)> = BinaryHeap::new();
        let mut best: Option<(u64, ChangeVersionHash)> = None;
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
                if best.is_none_or(|(g, h)| (gen, Reverse(hash)) > (g, Reverse(h))) {
                    best = Some((gen, hash));
                }
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
        Ok(best.map(|(_, h)| h))
    }
}
