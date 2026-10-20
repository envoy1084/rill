//! History walks for `log`.

use std::collections::{BinaryHeap, HashSet};

use rill_types::{ChangeInfo, ChangeVersionHash, Timestamp};

use super::commit_graph::CommitGraph;
use crate::error::Result;

/// One version in a log listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub info: ChangeInfo,
    pub generation: u64,
}

/// Heap key: higher generation first, then newer, then by hash so that
/// the order is total and reproducible.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Key(u64, Timestamp, ChangeVersionHash);

impl CommitGraph {
    /// Up to `limit` ancestors of `start` (inclusive), each before all of
    /// its parents, newest first among unrelated versions.
    pub async fn ancestors(
        &self,
        start: &ChangeVersionHash,
        limit: usize,
    ) -> Result<Vec<LogEntry>> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut heap = BinaryHeap::new();
        let first = self.info(start).await?;
        seen.insert(*start);
        heap.push(Key(self.generation(start).await?, first.created_at, *start));
        while out.len() < limit {
            let Some(Key(generation, _, hash)) = heap.pop() else {
                break;
            };
            let info = self.info(&hash).await?;
            for parent in &info.parents {
                if seen.insert(*parent) {
                    let p = self.info(parent).await?;
                    heap.push(Key(self.generation(parent).await?, p.created_at, *parent));
                }
            }
            out.push(LogEntry { info, generation });
        }
        Ok(out)
    }

    /// Ancestors of `start` that are not ancestors of `exclude`: the
    /// versions a change contributes on top of a base, in the same order
    /// as [`ancestors`](Self::ancestors).
    ///
    /// Both sides are walked together in generation order so that an
    /// uninteresting mark always reaches a version before it is emitted;
    /// the walk stops once nothing interesting remains queued.
    pub async fn ancestors_excluding(
        &self,
        start: &ChangeVersionHash,
        exclude: &ChangeVersionHash,
        limit: usize,
    ) -> Result<Vec<LogEntry>> {
        let mut out = Vec::new();
        let mut uninteresting: HashSet<ChangeVersionHash> = HashSet::new();
        let mut queued: HashSet<ChangeVersionHash> = HashSet::new();
        let mut heap = BinaryHeap::new();
        for (hash, excluded) in [(start, false), (exclude, true)] {
            let info = self.info(hash).await?;
            if excluded {
                uninteresting.insert(*hash);
            }
            if queued.insert(*hash) {
                heap.push(Key(self.generation(hash).await?, info.created_at, *hash));
            }
        }
        while out.len() < limit && heap.iter().any(|k| !uninteresting.contains(&k.2)) {
            let Some(Key(generation, _, hash)) = heap.pop() else {
                break;
            };
            let info = self.info(&hash).await?;
            let excluded = uninteresting.contains(&hash);
            for parent in &info.parents {
                if excluded {
                    uninteresting.insert(*parent);
                }
                if queued.insert(*parent) {
                    let p = self.info(parent).await?;
                    heap.push(Key(self.generation(parent).await?, p.created_at, *parent));
                }
            }
            if !excluded {
                out.push(LogEntry { info, generation });
            }
        }
        Ok(out)
    }
}
