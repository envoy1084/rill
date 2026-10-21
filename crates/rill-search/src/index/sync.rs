//! Keeping a [`SearchIndex`] and the store in step: loading, writing
//! (the one writer, holding the search lease) and refreshing (readers).

use std::collections::{BTreeMap, HashSet};

use rill_store::repo::Lease;

use super::docs::DocTable;
use super::persist::{self, shard_of, SHARD_COUNT};
use super::search_index::{FlushStats, SearchIndex};
use crate::error::Result;
use crate::trigram::{DocId, PostingList};

impl SearchIndex {
    /// Replace in-memory postings and the document table with the persisted
    /// ones. Unflushed state is discarded.
    pub async fn load(&self) -> Result<()> {
        let _sync = self.syncing.lock().await;
        let state = persist::get_state(&self.repo).await?;
        let docs = match persist::get_docs(&self.repo).await? {
            Some(record) => DocTable::from_record(record),
            None => DocTable::new(),
        };
        self.postings.clear();
        self.seen.clear();
        for shard in 0..SHARD_COUNT {
            self.load_shard(shard as u8).await?;
        }
        self.adopt_docs(docs);
        self.pending_markers.lock().clear();
        self.dirty_shards.lock().clear();
        *self.state.lock() = state;
        tracing::info!(docs = self.docs.read().len(), "search index loaded");
        Ok(())
    }

    /// Bring this process's copy up to what the writer last flushed,
    /// reloading only the shards and table whose generation moved; `true`
    /// when anything was reloaded. A process with unflushed writes of its
    /// own is the writer and already ahead, so it is left alone.
    pub async fn refresh(&self) -> Result<bool> {
        let _sync = self.syncing.lock().await;
        if !self.dirty_shards.lock().is_empty() || !self.pending_markers.lock().is_empty() {
            return Ok(false);
        }
        let stored = persist::get_state(&self.repo).await?;
        let loaded = self.state.lock().clone();
        if stored == loaded {
            return Ok(false);
        }
        if stored.docs != loaded.docs {
            if let Some(record) = persist::get_docs(&self.repo).await? {
                self.adopt_docs(DocTable::from_record(record));
            }
        }
        let mut reloaded = 0;
        for shard in 0..SHARD_COUNT as u16 {
            let shard = shard as u8;
            if stored.shard(shard) != loaded.shard(shard) {
                self.postings.retain(|t, _| shard_of(*t) != shard);
                self.load_shard(shard).await?;
                reloaded += 1;
            }
        }
        tracing::debug!(
            shards = reloaded,
            docs = self.docs.read().len(),
            "search index refreshed"
        );
        *self.state.lock() = stored;
        Ok(true)
    }

    /// Persist dirty shards, the document table and pending blob markers,
    /// in that order (see `persist`), then the new generations. Every write
    /// is fenced on `fence` when given.
    pub async fn flush(&self, fence: Option<&Lease>) -> Result<FlushStats> {
        let _sync = self.syncing.lock().await;
        let dirty: Vec<u8> = std::mem::take(&mut *self.dirty_shards.lock())
            .into_iter()
            .collect();
        let markers = std::mem::take(&mut *self.pending_markers.lock());
        let mut stats = FlushStats::default();
        // A blob too short to have trigrams still gets a document id, so
        // the table can change without any shard becoming dirty.
        let docs_dirty = !dirty.is_empty() || markers.iter().any(|(_, d)| d.is_some());
        if dirty.is_empty() && markers.is_empty() {
            return Ok(stats);
        }
        let mut state = self.state.lock().clone();
        if !dirty.is_empty() {
            let wanted: HashSet<u8> = dirty.iter().copied().collect();
            let mut by_shard: BTreeMap<u8, Vec<(u32, PostingList)>> = BTreeMap::new();
            for entry in self.postings.iter() {
                let shard = shard_of(*entry.key());
                if wanted.contains(&shard) {
                    by_shard
                        .entry(shard)
                        .or_default()
                        .push((*entry.key(), entry.value().clone()));
                }
            }
            for (shard, mut lists) in by_shard {
                lists.sort_by_key(|(t, _)| *t);
                let refs = lists.iter().map(|(t, l)| (*t, l)).collect();
                persist::put_shard(&self.repo, shard, refs, fence).await?;
                state.bump_shard(shard);
                stats.shards += 1;
            }
        }
        if docs_dirty {
            let record = self.docs.read().to_record();
            persist::put_docs(&self.repo, &record, fence).await?;
            state.docs += 1;
        }
        for (hash, doc) in markers {
            persist::put_marker(&self.repo, &hash, persist::BlobMarker { doc }, fence).await?;
            stats.markers += 1;
        }
        persist::put_state(&self.repo, &state, fence).await?;
        *self.state.lock() = state;
        tracing::debug!(?stats, "search index flushed");
        Ok(stats)
    }

    async fn load_shard(&self, shard: u8) -> Result<()> {
        if let Some(lists) = persist::get_shard(&self.repo, shard).await? {
            for (trigram, list) in lists {
                self.postings.insert(trigram, list);
            }
        }
        Ok(())
    }

    fn adopt_docs(&self, docs: DocTable) {
        for id in 0..docs.len() as DocId {
            if let Some(hash) = docs.hash_of(id) {
                self.seen.insert(hash, Some(id));
            }
        }
        *self.docs.write() = docs;
    }
}
