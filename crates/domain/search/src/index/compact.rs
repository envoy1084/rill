//! Merging small segments into larger ones, tier by tier: once a shard has
//! `merge_fanout` segments of one level, they become one segment of the
//! next level, and the segment list swaps them in one write.

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
                let merged = self
                    .store_segment(bytes, docs, level.saturating_add(1))
                    .await?;
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
