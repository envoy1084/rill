//! The raw ordered key-value engine each metadata backend provides.
//!
//! The operations mirror FoundationDB's: point and range reads, blind
//! writes, range clears, atomic add and versionstamped keys. The keyspace
//! layer builds every [`MetaStore`](crate::MetaStore) rule on these, so a
//! backend only has to get this small surface right.

use async_trait::async_trait;
use bytes::Bytes;

use crate::error::{Error, Result};

/// A raw scan over `[start, end)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRange {
    pub start: Bytes,
    pub end: Bytes,
    pub limit: usize,
    pub reverse: bool,
}

/// A store of raw keys and values with interactive transactions.
#[async_trait]
pub trait Engine: Send + Sync + 'static {
    type Txn: EngineTxn;

    /// Start a transaction. Its reads are serializable with every other
    /// transaction's writes.
    async fn begin(&self) -> Result<Self::Txn>;

    /// Read one key outside any transaction.
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>>;

    /// Scan outside any transaction. One call is a consistent snapshot.
    async fn scan(&self, range: &RawRange) -> Result<Vec<(Bytes, Bytes)>>;

    /// Return once the value at `key` is no longer `seen`, or after
    /// `timeout`. The default polls; an engine that can watch a key
    /// overrides it.
    async fn wait_change(
        &self,
        key: &[u8],
        seen: Option<Bytes>,
        timeout: std::time::Duration,
    ) -> Result<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.get(key).await? != seen || tokio::time::Instant::now() >= deadline {
                return Ok(());
            }
            tokio::time::sleep(crate::meta::POLL_INTERVAL.min(timeout)).await;
        }
    }
}

/// One transaction. Reads may not observe the transaction's own writes;
/// the keyspace layer finishes reading before it writes.
#[async_trait]
pub trait EngineTxn: Send {
    async fn get(&mut self, key: &[u8]) -> Result<Option<Bytes>>;

    async fn scan(&mut self, range: &RawRange) -> Result<Vec<(Bytes, Bytes)>>;

    fn set(&mut self, key: Vec<u8>, value: Bytes);

    fn clear(&mut self, key: Vec<u8>);

    fn clear_range(&mut self, start: Vec<u8>, end: Vec<u8>);

    /// Add `delta` to the little-endian `i64` at `key`; absent counts as 0.
    fn add(&mut self, key: Vec<u8>, delta: i64);

    /// Write a key holding a placeholder for this commit's 10-byte stamp.
    /// `key` ends with the placeholder's 4-byte little-endian offset, as
    /// FoundationDB's `SET_VERSIONSTAMPED_KEY` expects.
    fn set_versionstamped_key(&mut self, key: Vec<u8>, value: Bytes);

    /// Commit. Returns the commit's stamp when the transaction wrote a
    /// stamped key. Fails with [`Error::Retryable`] when nothing applied
    /// and the work may be redone, or [`Error::CommitUnknown`].
    async fn commit(&mut self) -> Result<Option<[u8; 10]>>;

    /// Back off after a retryable error and reset for another attempt.
    async fn reset_after(&mut self, error: &Error) -> Result<()>;
}

/// Replace the placeholder in a versionstamped key with `stamp` and drop
/// the trailing offset, as the database does on commit.
pub fn fill_stamp(key: &[u8], stamp: &[u8; 10]) -> Result<Vec<u8>> {
    let malformed = || Error::Codec("malformed versionstamped key".into());
    let split = key.len().checked_sub(4).ok_or_else(malformed)?;
    let (body, offset) = key.split_at(split);
    let offset = u32::from_le_bytes(offset.try_into().map_err(|_| malformed())?) as usize;
    let end = offset.checked_add(10).ok_or_else(malformed)?;
    let mut out = body.to_vec();
    out.get_mut(offset..end)
        .ok_or_else(malformed)?
        .copy_from_slice(stamp);
    Ok(out)
}

/// `old + delta` as FoundationDB's `ADD` computes it on 8-byte values.
pub fn add_le(old: Option<&[u8]>, delta: i64) -> [u8; 8] {
    let mut current = [0u8; 8];
    if let Some(old) = old {
        let n = old.len().min(8);
        current[..n].copy_from_slice(&old[..n]);
    }
    i64::from_le_bytes(current)
        .wrapping_add(delta)
        .to_le_bytes()
}
