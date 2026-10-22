//! Persistence of the index as derived data.
//!
//! Everything lives under the repository's derived-data keyspace:
//!
//! * `idx:blob@1` keyed by content hash: whether the blob was indexed (and
//!   under which document id) or skipped.
//! * `idx:root@1` keyed by root tree hash: the [`super::RootManifest`].
//! * `idx:docs@1` under the zero key: the document table.
//! * `idx:post@1` keyed by shard (trigram `>> 16`, so at most 256 shards):
//!   every posting list of that shard.
//! * `idx:state@1` under the zero key: the generation of each shard and of
//!   the document table, written last by every flush, so a reader reloads
//!   only what changed.
//!
//! One process writes at a time, holding the repository's search lease;
//! every write is fenced on it.
//!
//! Shards are rewritten whole on every flush. That keeps the format trivial
//! and the write O(shard), which is fine while an index fits in memory; the
//! upgrade path for larger repositories is append-only segments per shard
//! with background merges, which this layout does not preclude because a
//! reader only needs the union of a shard's lists.

use bytes::Bytes;
use rill_store::repo::{decode_tagged, encode_tagged, Lease};
use rill_store::RepoStore;
use rill_types::ObjectHash;
use serde::{Deserialize, Serialize};

use super::docs::DocTableRecord;
use crate::error::{Error, Result};
use crate::trigram::{DocId, PostingList};

/// Derived kind of per-blob index markers.
pub const KIND_BLOB: &str = "idx:blob@1";
/// Derived kind of per-root manifests.
pub const KIND_ROOT: &str = "idx:root@1";
/// Derived kind of the document table.
pub const KIND_DOCS: &str = "idx:docs@1";
/// Derived kind of posting shards.
pub const KIND_POST: &str = "idx:post@1";
/// Derived kind of the index state.
pub const KIND_STATE: &str = "idx:state@1";

/// Number of posting shards.
pub const SHARD_COUNT: usize = 256;

/// Persisted state of one blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobMarker {
    /// Document id, or `None` when the blob was skipped (binary or too big).
    pub doc: Option<DocId>,
}

/// Persisted form of one shard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShardRecord {
    /// `(trigram, encoded posting list)` pairs.
    pub lists: Vec<(u32, Vec<u8>)>,
}

/// Generations of what a flush wrote: one per shard (missing entries are
/// zero) and one for the document table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexState {
    pub shards: Vec<u64>,
    pub docs: u64,
}

impl IndexState {
    pub fn shard(&self, shard: u8) -> u64 {
        self.shards.get(shard as usize).copied().unwrap_or(0)
    }

    pub fn bump_shard(&mut self, shard: u8) {
        if self.shards.len() < SHARD_COUNT {
            self.shards.resize(SHARD_COUNT, 0);
        }
        self.shards[shard as usize] += 1;
    }
}

/// Shard a trigram belongs to.
pub fn shard_of(trigram: u32) -> u8 {
    (trigram >> 16) as u8
}

/// Key under which a shard is stored.
pub fn shard_key(shard: u8) -> ObjectHash {
    let mut bytes = [0u8; 32];
    bytes[0] = shard;
    ObjectHash::from_bytes(bytes)
}

/// Write a derived value, fenced when `fence` is given.
pub async fn put(
    repo: &RepoStore,
    kind: &str,
    key: &ObjectHash,
    value: Bytes,
    fence: Option<&Lease>,
) -> Result<()> {
    match fence {
        Some(lease) => repo.put_derived_fenced(kind, key, value, lease).await?,
        None => repo.put_derived(kind, key, value).await?,
    }
    Ok(())
}

/// Write one shard.
pub async fn put_shard(
    repo: &RepoStore,
    shard: u8,
    lists: Vec<(u32, &PostingList)>,
    fence: Option<&Lease>,
) -> Result<()> {
    let record = ShardRecord {
        lists: lists
            .into_iter()
            .map(|(t, l)| {
                let mut bytes = Vec::with_capacity(l.serialized_size());
                l.serialize_into(&mut bytes)
                    .map_err(|e| Error::Codec(format!("posting list: {e}")))?;
                Ok((t, bytes))
            })
            .collect::<Result<_>>()?,
    };
    put(
        repo,
        KIND_POST,
        &shard_key(shard),
        encode(KIND_POST, &record)?,
        fence,
    )
    .await
}

/// Write the index state.
pub async fn put_state(repo: &RepoStore, state: &IndexState, fence: Option<&Lease>) -> Result<()> {
    put(
        repo,
        KIND_STATE,
        &ObjectHash::ZERO,
        encode(KIND_STATE, state)?,
        fence,
    )
    .await
}

/// Read the index state; an index never flushed has the default state.
pub async fn get_state(repo: &RepoStore) -> Result<IndexState> {
    match repo.get_derived(KIND_STATE, &ObjectHash::ZERO).await? {
        Some(bytes) => decode(KIND_STATE, &bytes),
        None => Ok(IndexState::default()),
    }
}

/// Read one shard's decoded posting lists, if stored.
pub async fn get_shard(repo: &RepoStore, shard: u8) -> Result<Option<Vec<(u32, PostingList)>>> {
    let Some(bytes) = repo.get_derived(KIND_POST, &shard_key(shard)).await? else {
        return Ok(None);
    };
    let record: ShardRecord = decode(KIND_POST, &bytes)?;
    record
        .lists
        .into_iter()
        .map(|(t, bytes)| {
            let list = PostingList::deserialize_from(&bytes[..])
                .map_err(|e| Error::Codec(format!("posting list: {e}")))?;
            Ok((t, list))
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

/// Write the document table.
pub async fn put_docs(
    repo: &RepoStore,
    record: &DocTableRecord,
    fence: Option<&Lease>,
) -> Result<()> {
    put(
        repo,
        KIND_DOCS,
        &ObjectHash::ZERO,
        encode(KIND_DOCS, record)?,
        fence,
    )
    .await
}

/// Read the document table, if stored.
pub async fn get_docs(repo: &RepoStore) -> Result<Option<DocTableRecord>> {
    match repo.get_derived(KIND_DOCS, &ObjectHash::ZERO).await? {
        Some(bytes) => Ok(Some(decode(KIND_DOCS, &bytes)?)),
        None => Ok(None),
    }
}

/// Write a blob marker.
pub async fn put_marker(
    repo: &RepoStore,
    hash: &ObjectHash,
    marker: BlobMarker,
    fence: Option<&Lease>,
) -> Result<()> {
    put(repo, KIND_BLOB, hash, encode(KIND_BLOB, &marker)?, fence).await
}

/// Read a blob marker, if stored.
pub async fn get_marker(repo: &RepoStore, hash: &ObjectHash) -> Result<Option<BlobMarker>> {
    match repo.get_derived(KIND_BLOB, hash).await? {
        Some(bytes) => Ok(Some(decode(KIND_BLOB, &bytes)?)),
        None => Ok(None),
    }
}

/// Encode a record under its storage kind as the tag.
pub fn encode<T: Serialize>(kind: &str, value: &T) -> Result<Bytes> {
    Ok(encode_tagged(kind, value)?)
}

/// Decode a record written by [`encode`] under the same kind.
pub fn decode<T: serde::de::DeserializeOwned>(kind: &str, bytes: &[u8]) -> Result<T> {
    Ok(decode_tagged(kind, bytes)?)
}
