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
//!
//! Shards are rewritten whole on every flush. That keeps the format trivial
//! and the write O(shard), which is fine while an index fits in memory; the
//! upgrade path for larger repositories is append-only segments per shard
//! with background merges, which this layout does not preclude because a
//! reader only needs the union of a shard's lists.

use bytes::Bytes;
use rill_store::repo::{decode_record, encode_record};
use rill_store::RepoStore;
use rill_types::ObjectHash;
use serde::{Deserialize, Serialize};

use super::docs::DocTableRecord;
use crate::error::Result;
use crate::trigram::{DocId, PostingList};

/// Derived kind of per-blob index markers.
pub const KIND_BLOB: &str = "idx:blob@1";
/// Derived kind of per-root manifests.
pub const KIND_ROOT: &str = "idx:root@1";
/// Derived kind of the document table.
pub const KIND_DOCS: &str = "idx:docs@1";
/// Derived kind of posting shards.
pub const KIND_POST: &str = "idx:post@1";

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

/// Write one shard.
pub async fn put_shard(repo: &RepoStore, shard: u8, lists: Vec<(u32, &PostingList)>) -> Result<()> {
    let record = ShardRecord {
        lists: lists.into_iter().map(|(t, l)| (t, l.encode())).collect(),
    };
    repo.put_derived(KIND_POST, &shard_key(shard), encode(&record)?)
        .await?;
    Ok(())
}

/// Read one shard's decoded posting lists, if stored.
pub async fn get_shard(repo: &RepoStore, shard: u8) -> Result<Option<Vec<(u32, PostingList)>>> {
    let Some(bytes) = repo.get_derived(KIND_POST, &shard_key(shard)).await? else {
        return Ok(None);
    };
    let record: ShardRecord = decode(&bytes)?;
    record
        .lists
        .into_iter()
        .map(|(t, bytes)| Ok((t, PostingList::decode(&bytes)?)))
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

/// Write the document table.
pub async fn put_docs(repo: &RepoStore, record: &DocTableRecord) -> Result<()> {
    repo.put_derived(KIND_DOCS, &ObjectHash::ZERO, encode(record)?)
        .await?;
    Ok(())
}

/// Read the document table, if stored.
pub async fn get_docs(repo: &RepoStore) -> Result<Option<DocTableRecord>> {
    match repo.get_derived(KIND_DOCS, &ObjectHash::ZERO).await? {
        Some(bytes) => Ok(Some(decode(&bytes)?)),
        None => Ok(None),
    }
}

/// Write a blob marker.
pub async fn put_marker(repo: &RepoStore, hash: &ObjectHash, marker: BlobMarker) -> Result<()> {
    repo.put_derived(KIND_BLOB, hash, encode(&marker)?).await?;
    Ok(())
}

/// Read a blob marker, if stored.
pub async fn get_marker(repo: &RepoStore, hash: &ObjectHash) -> Result<Option<BlobMarker>> {
    match repo.get_derived(KIND_BLOB, hash).await? {
        Some(bytes) => Ok(Some(decode(&bytes)?)),
        None => Ok(None),
    }
}

/// Encode a record with the store's versioned record framing.
pub fn encode<T: Serialize>(value: &T) -> Result<Bytes> {
    Ok(encode_record(value)?)
}

/// Decode a record written by [`encode`].
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    Ok(decode_record(bytes)?)
}
