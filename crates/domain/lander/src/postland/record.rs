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
/// Misc row holding the worker's [`Marks`].
pub const POSTLAND_MARKS_MISC: &str = "postland_marks";
/// Number of records kept in the ring.
pub const POSTLAND_RING: usize = 1000;

/// Verification status of a trunk position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    Green,
    Red,
    /// Verified more than once with differing results, or failed only in
    /// targets that passed when run again. Counts as passing.
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
    /// What failed, for a directly verified red position.
    pub failures: Vec<crate::traits::Failure>,
}

impl VerificationRecord {
    /// True for a green record.
    pub fn is_green(&self) -> bool {
        self.status == VerificationStatus::Green
    }

    /// Whether the position passed: green, or flaky (it failed only in
    /// targets that then passed when run again).
    pub fn passed(&self) -> bool {
        matches!(
            self.status,
            VerificationStatus::Green | VerificationStatus::Flaky
        )
    }
}

/// The last positions known to pass: verifying from there skips the
/// steps nothing since has touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Marks {
    /// Last position that passed.
    pub green: TrunkSeq,
    /// Last position that passed the full plan.
    pub full_green: TrunkSeq,
}

pub async fn load_marks(repo: &RepoStore) -> Result<Marks> {
    match repo.get_misc(POSTLAND_MARKS_MISC).await? {
        Some(bytes) => decode(POSTLAND_MARKS_MISC, &bytes),
        None => Ok(Marks::default()),
    }
}

/// Move the marks from `from` to `to`, fenced by `lease`.
pub async fn store_marks(
    repo: &RepoStore,
    from: Marks,
    to: Marks,
    lease: &rill_store::repo::Lease,
) -> Result<()> {
    if from == to {
        return Ok(());
    }
    let expected = match from == Marks::default() {
        true => repo.get_misc(POSTLAND_MARKS_MISC).await?,
        false => Some(encode(POSTLAND_MARKS_MISC, &from)?),
    };
    repo.set_misc_if(
        POSTLAND_MARKS_MISC,
        expected,
        encode(POSTLAND_MARKS_MISC, &to)?,
        lease,
    )
    .await?;
    Ok(())
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

/// Move the cursor from `from` to `to`; fails once `lease` is lost or the
/// cursor is no longer `from`, so a replaced or racing verifier cannot
/// move it twice.
pub async fn store_cursor(
    repo: &RepoStore,
    from: TrunkSeq,
    to: TrunkSeq,
    lease: &rill_store::repo::Lease,
) -> Result<()> {
    let expected =
        (from != TrunkSeq::ZERO).then(|| Bytes::copy_from_slice(&from.get().to_be_bytes()));
    repo.set_misc_if(
        POSTLAND_CURSOR_MISC,
        expected,
        Bytes::copy_from_slice(&to.get().to_be_bytes()),
        lease,
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
