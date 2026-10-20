//! Post-land verification records and the worker's cursor.

use bytes::Bytes;
use rill_store::repo::{decode_tagged, encode_tagged};
use rill_store::RepoStore;
use rill_types::{ChangeVersionHash, Timestamp, TrunkSeq};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Derived-data kind of per-version verification records.
pub const POSTLAND_KIND: &str = "postland@1";
/// Misc row holding the ring of recent records.
pub const POSTLAND_INDEX_MISC: &str = "postland_index";
/// Misc row holding the last verified trunk sequence.
pub const POSTLAND_CURSOR_MISC: &str = "postland_cursor";
/// Number of records kept in the ring.
pub const POSTLAND_RING: usize = 1000;

/// Verification status of a trunk position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    Green,
    Red,
    /// Verified more than once with differing results.
    Flaky,
}

/// What post-land verification concluded about a trunk position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationRecord {
    pub seq: TrunkSeq,
    pub version: ChangeVersionHash,
    pub status: VerificationStatus,
    pub details: String,
    /// The first red position responsible, for red records. A red
    /// record's own `seq` when it is the culprit.
    pub culprit: Option<TrunkSeq>,
    pub at: Timestamp,
    /// True when the status was inferred from neighbours in a batch
    /// rather than verified directly.
    pub inferred: bool,
}

impl VerificationRecord {
    /// True for a green record.
    pub fn is_green(&self) -> bool {
        self.status == VerificationStatus::Green
    }
}

/// Store a record under its version and append it to the ring.
pub async fn put_record(repo: &RepoStore, record: &VerificationRecord) -> Result<()> {
    repo.put_derived(
        POSTLAND_KIND,
        &record.version,
        encode(POSTLAND_KIND, record)?,
    )
    .await?;
    let mut ring = recent_records(repo).await?;
    ring.retain(|r| r.seq != record.seq);
    ring.push(record.clone());
    if ring.len() > POSTLAND_RING {
        let excess = ring.len() - POSTLAND_RING;
        ring.drain(..excess);
    }
    repo.set_misc(POSTLAND_INDEX_MISC, encode(POSTLAND_INDEX_MISC, &ring)?)
        .await?;
    Ok(())
}

/// The record for a landed version, if any.
pub async fn record_of(
    repo: &RepoStore,
    version: &ChangeVersionHash,
) -> Result<Option<VerificationRecord>> {
    match repo.get_derived(POSTLAND_KIND, version).await? {
        Some(bytes) => Ok(Some(decode(POSTLAND_KIND, &bytes)?)),
        None => Ok(None),
    }
}

/// The record for a trunk position, if any.
pub async fn record_at(repo: &RepoStore, seq: TrunkSeq) -> Result<Option<VerificationRecord>> {
    match repo.trunk_at(seq).await? {
        Some(hash) => record_of(repo, &hash).await,
        None => Ok(None),
    }
}

/// Recent records in sequence order, up to [`POSTLAND_RING`].
pub async fn recent_records(repo: &RepoStore) -> Result<Vec<VerificationRecord>> {
    let mut records: Vec<VerificationRecord> = match repo.get_misc(POSTLAND_INDEX_MISC).await? {
        Some(bytes) => decode(POSTLAND_INDEX_MISC, &bytes)?,
        None => Vec::new(),
    };
    records.sort_by_key(|r| r.seq);
    Ok(records)
}

/// The last trunk position the worker has verified through.
pub async fn load_cursor(repo: &RepoStore) -> Result<TrunkSeq> {
    match repo.get_misc(POSTLAND_CURSOR_MISC).await? {
        Some(bytes) => {
            let arr: [u8; 8] = bytes
                .as_ref()
                .try_into()
                .map_err(|_| Error::Codec("bad postland cursor".into()))?;
            Ok(TrunkSeq(u64::from_be_bytes(arr)))
        }
        None => Ok(TrunkSeq::ZERO),
    }
}

/// Persist the cursor.
pub async fn store_cursor(repo: &RepoStore, seq: TrunkSeq) -> Result<()> {
    repo.set_misc(
        POSTLAND_CURSOR_MISC,
        Bytes::copy_from_slice(&seq.get().to_be_bytes()),
    )
    .await?;
    Ok(())
}

fn encode<T: Serialize>(tag: &str, value: &T) -> Result<Bytes> {
    encode_tagged(tag, value).map_err(|e| Error::Codec(e.to_string()))
}

fn decode<T: serde::de::DeserializeOwned>(tag: &str, bytes: &[u8]) -> Result<T> {
    decode_tagged(tag, bytes).map_err(|e| Error::Codec(e.to_string()))
}
