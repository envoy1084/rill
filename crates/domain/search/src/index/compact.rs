//! Merging small segments into larger ones, tier by tier. A segment's
//! level is its size tier (under 1 MiB × fanout is level 0, each further
//! factor of `merge_fanout` one more), so a landing's tiny segments merge
//! with each other, never with a full index's large ones. Once a shard has
//! `merge_fanout` segments of one level, they become one segment, and the
//! segment list swaps them in one write.

use rill_store::repo::Lease;

use super::records::{self, SegmentRef};
use super::search_index::SearchIndex;
use crate::error::Result;
use crate::segment::merge_segments;

/// What a compaction did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactStats {
    pub merges: usize,
    pub segments_before: usize,
    pub segments_after: usize,
}

/// The smallest size a tier spans, before the first factor of the fanout.
const TIER_BASE: u64 = 1024 * 1024;

/// The size tier of a segment of `bytes`.
pub(super) fn tier(bytes: u64, fanout: usize) -> u8 {
    let fanout = fanout.max(2) as u64;
    let mut level = 0u8;
    let mut cap = TIER_BASE.saturating_mul(fanout);
    while bytes >= cap && level < u8::MAX {
        level += 1;
        cap = cap.saturating_mul(fanout);
    }
    level
}

impl SearchIndex {
    /// Merge segments in every shard until no tier is full.
    pub async fn compact(&self, fence: Option<&Lease>) -> Result<CompactStats> {
        let _guard = self.writing.lock().await;
        let mut set = self.set.lock().clone();
        let fanout = self.config.merge_fanout.max(2);
        let mut stats = CompactStats {
            segments_before: set.shards.iter().map(Vec::len).sum(),
            ..CompactStats::default()
        };
        for shard in 0..set.shards.len() {
            loop {
                let refs = &set.shards[shard];
                let Some(level) =
                    (0..=u8::MAX).find(|l| refs.iter().filter(|r| r.level == *l).count() >= fanout)
                else {
                    break;
                };
                let mut group: Vec<SegmentRef> =
                    refs.iter().filter(|r| r.level == level).copied().collect();
                group.sort_by_key(|r| r.bytes);
                group.truncate(fanout);
                let mut open = Vec::with_capacity(group.len());
                for r in &group {
                    open.push(self.segments.get(&self.repo, r).await?);
                }
                let docs: u32 = open.iter().map(|s| s.docs().len() as u32).sum();
                let bytes = tokio::task::spawn_blocking(move || {
                    let refs: Vec<_> = open.iter().map(|s| s.as_ref()).collect();
                    merge_segments(&refs)
                })
                .await??;
                let merged = self.store_segment(bytes, docs).await?;
                let gone: Vec<_> = group.iter().map(|r| r.hash).collect();
                set.shards[shard].retain(|r| !gone.contains(&r.hash));
                set.shards[shard].push(merged);
                stats.merges += 1;
            }
        }
        if stats.merges > 0 {
            set.generation += 1;
            records::put_segment_set(&self.repo, &set, fence).await?;
            self.install(set.clone()).await?;
        }
        stats.segments_after = set.shards.iter().map(Vec::len).sum();
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::tier;

    #[test]
    fn tiers_grow_by_the_fanout() {
        assert_eq!(tier(0, 8), 0);
        assert_eq!(tier(8 * 1024 * 1024 - 1, 8), 0);
        assert_eq!(tier(8 * 1024 * 1024, 8), 1);
        assert_eq!(tier(64 * 1024 * 1024, 8), 2);
    }
}
