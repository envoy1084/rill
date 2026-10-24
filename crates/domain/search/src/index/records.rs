//! What the index stores as derived data of the repository:
//!
//! * `idx:marker@1` by content hash: whether a blob is in a segment, or was
//!   skipped (binary or too large).
//! * `idx:paths@1` by root tree: the root's [`PathLayer`].
//! * `idx:segments@1` under the zero key: every shard's segment list and how
//!   far trunk is indexed. Written last by every index job, so a reader that
//!   sees a list sees its segments, and a root's layer is written only after
//!   the segments holding its new blobs.
//!
//! Segments themselves are objects (`ObjectKind::Derived`). One process
//! writes at a time, holding the repository's search lease; every write is
//! fenced on it.

use bytes::Bytes;
use rill_store::repo::{decode_tagged, encode_tagged, Lease};
use rill_store::RepoStore;
use rill_types::{ObjectHash, TreeHash, TrunkSeq};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths::PathLayer;

pub const KIND_MARKER: &str = "idx:marker@1";
pub const KIND_PATHS: &str = "idx:paths@1";
pub const KIND_SEGMENTS: &str = "idx:segments@1";

/// A segment in a shard's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentRef {
    /// The segment object's hash.
    pub hash: ObjectHash,
    pub docs: u32,
    pub bytes: u64,
    /// Size tier: 0 for a fresh segment, one more per merge.
    pub level: u8,
}

/// How far trunk is indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Through {
    pub seq: TrunkSeq,
    pub root: TreeHash,
}

/// Every shard's segments.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentSet {
    /// Bumped by every write, so readers reload only on change.
    pub generation: u64,
    pub shards: Vec<Vec<SegmentRef>>,
    pub through: Option<Through>,
}

async fn put(
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

async fn get<T: serde::de::DeserializeOwned>(
    repo: &RepoStore,
    kind: &str,
    key: &ObjectHash,
) -> Result<Option<T>> {
    match repo.get_derived(kind, key).await? {
        Some(bytes) => Ok(Some(decode_tagged(kind, &bytes)?)),
        None => Ok(None),
    }
}

/// Whether each blob was indexed (`true`) or skipped (`false`); `None`
/// when unknown.
pub async fn markers(repo: &RepoStore, hashes: &[ObjectHash]) -> Result<Vec<Option<bool>>> {
    repo.get_derived_many(KIND_MARKER, hashes)
        .await?
        .into_iter()
        .map(|v| {
            v.map(|b| decode_tagged(KIND_MARKER, &b))
                .transpose()
                .map_err(Into::into)
        })
        .collect()
}

/// Store markers in one transaction.
pub async fn put_markers(
    repo: &RepoStore,
    markers: &[(ObjectHash, bool)],
    fence: Option<&Lease>,
) -> Result<()> {
    let values = markers
        .iter()
        .map(|(h, indexed)| Ok((*h, encode_tagged(KIND_MARKER, indexed)?)))
        .collect::<Result<Vec<_>>>()?;
    repo.put_derived_many(KIND_MARKER, &values, fence).await?;
    Ok(())
}

/// A root's layer, decoded off the async threads (a base holds every
/// file of the root).
pub async fn layer(repo: &RepoStore, root: &TreeHash) -> Result<Option<PathLayer>> {
    match repo.get_derived(KIND_PATHS, root).await? {
        Some(bytes) => Ok(Some(
            tokio::task::spawn_blocking(move || decode_tagged(KIND_PATHS, &bytes)).await??,
        )),
        None => Ok(None),
    }
}

pub async fn put_layer(
    repo: &RepoStore,
    root: &TreeHash,
    layer: &PathLayer,
    fence: Option<&Lease>,
) -> Result<()> {
    put(
        repo,
        KIND_PATHS,
        root,
        encode_tagged(KIND_PATHS, layer)?,
        fence,
    )
    .await
}

pub async fn segment_set(repo: &RepoStore) -> Result<SegmentSet> {
    Ok(get(repo, KIND_SEGMENTS, &ObjectHash::ZERO)
        .await?
        .unwrap_or_default())
}

pub async fn put_segment_set(
    repo: &RepoStore,
    set: &SegmentSet,
    fence: Option<&Lease>,
) -> Result<()> {
    put(
        repo,
        KIND_SEGMENTS,
        &ObjectHash::ZERO,
        encode_tagged(KIND_SEGMENTS, set)?,
        fence,
    )
    .await
}
