//! The FoundationDB engine.
//!
//! Every key lives under a root prefix, so one cluster can hold several
//! deployments (or test runs) side by side.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use foundationdb::options::{DatabaseOption, MutationType, StreamingMode};
use foundationdb::{
    Database, FdbError, KeySelector, RangeOption, Transaction, TransactionCommitError,
};

use crate::error::{Error, Result};
use crate::keyspace::{Engine, EngineTxn, RawRange};

/// FoundationDB's own limit on a transaction's lifetime.
const TX_TIMEOUT_MS: i32 = 5_000;

/// An engine over a FoundationDB database.
pub struct FdbEngine {
    db: Arc<Database>,
    root: Arc<[u8]>,
}

impl FdbEngine {
    /// An engine over `db` keeping every key under `root`.
    pub fn new(db: Database, root: &[u8]) -> Result<Self> {
        db.set_option(DatabaseOption::TransactionTimeout(TX_TIMEOUT_MS))
            .map_err(backend)?;
        Ok(FdbEngine {
            db: Arc::new(db),
            root: Arc::from(root),
        })
    }

    /// Delete every key under the root. For tests and teardown.
    pub async fn clear_all(&self) -> Result<()> {
        let trx = self.db.create_trx().map_err(backend)?;
        let (start, end) = self.bounds(&[], None);
        trx.clear_range(&start, &end);
        trx.commit().await.map_err(|e| backend(*e))?;
        Ok(())
    }

    fn bounds(&self, start: &[u8], end: Option<&[u8]>) -> (Vec<u8>, Vec<u8>) {
        let start = prefixed(&self.root, start);
        let end = match end {
            Some(end) => prefixed(&self.root, end),
            None => {
                let mut e = self.root.to_vec();
                e.push(0xFF);
                e
            }
        };
        (start, end)
    }
}

fn prefixed(root: &[u8], key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(root.len() + key.len());
    out.extend_from_slice(root);
    out.extend_from_slice(key);
    out
}

fn backend(e: FdbError) -> Error {
    Error::Backend(format!("foundationdb {}: {}", e.code(), e.message()))
}

/// Classify an error from a read or a commit.
fn classify(e: FdbError, committing: bool) -> Error {
    if committing && e.is_maybe_committed() {
        Error::CommitUnknown(format!("foundationdb {}: {}", e.code(), e.message()))
    } else if e.is_retryable() {
        Error::Retryable(format!("foundationdb {}: {}", e.code(), e.message()))
    } else {
        backend(e)
    }
}

async fn scan_in(
    trx: &Transaction,
    root: &[u8],
    range: &RawRange,
) -> std::result::Result<Vec<(Bytes, Bytes)>, FdbError> {
    let mut out = Vec::new();
    if range.limit == 0 || range.start >= range.end {
        return Ok(out);
    }
    let start = prefixed(root, &range.start);
    let end = prefixed(root, &range.end);
    let mut opt = Some(RangeOption {
        limit: (range.limit != usize::MAX).then_some(range.limit),
        mode: StreamingMode::WantAll,
        reverse: range.reverse,
        ..RangeOption::from((
            KeySelector::first_greater_or_equal(start),
            KeySelector::first_greater_or_equal(end),
        ))
    });
    while let Some(current) = opt.take() {
        let values = trx.get_range(&current, 1, false).await?;
        for kv in &values {
            out.push((
                Bytes::copy_from_slice(&kv.key()[root.len()..]),
                Bytes::copy_from_slice(kv.value()),
            ));
        }
        opt = current.next_range(&values);
    }
    Ok(out)
}

#[async_trait]
impl Engine for FdbEngine {
    type Txn = FdbTxn;

    async fn begin(&self) -> Result<FdbTxn> {
        Ok(FdbTxn {
            state: State::Open(self.db.create_trx().map_err(backend)?),
            root: Arc::clone(&self.root),
            failure: None,
            stamped: false,
        })
    }

    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let trx = self.db.create_trx().map_err(backend)?;
        let value = trx
            .get(&prefixed(&self.root, key), false)
            .await
            .map_err(|e| classify(e, false))?;
        Ok(value.map(|v| Bytes::copy_from_slice(&v)))
    }

    async fn scan(&self, range: &RawRange) -> Result<Vec<(Bytes, Bytes)>> {
        let trx = self.db.create_trx().map_err(backend)?;
        scan_in(&trx, &self.root, range)
            .await
            .map_err(|e| classify(e, false))
    }
}

enum State {
    Open(Transaction),
    Failed(TransactionCommitError),
    Gone,
}

/// One FoundationDB transaction.
pub struct FdbTxn {
    state: State,
    root: Arc<[u8]>,
    /// The error a read failed with, kept for `reset_after`.
    failure: Option<FdbError>,
    stamped: bool,
}

impl FdbTxn {
    fn trx(&self) -> Result<&Transaction> {
        match &self.state {
            State::Open(trx) => Ok(trx),
            _ => Err(Error::Backend("transaction used after commit".into())),
        }
    }

    fn key(&self, key: &[u8]) -> Vec<u8> {
        prefixed(&self.root, key)
    }

    fn read_failed(&mut self, e: FdbError) -> Error {
        let error = classify(e, false);
        self.failure = Some(e);
        error
    }
}

#[async_trait]
impl EngineTxn for FdbTxn {
    async fn get(&mut self, key: &[u8]) -> Result<Option<Bytes>> {
        let key = self.key(key);
        let result = self.trx()?.get(&key, false).await;
        match result {
            Ok(value) => Ok(value.map(|v| Bytes::copy_from_slice(&v))),
            Err(e) => Err(self.read_failed(e)),
        }
    }

    async fn scan(&mut self, range: &RawRange) -> Result<Vec<(Bytes, Bytes)>> {
        let root = Arc::clone(&self.root);
        let result = scan_in(self.trx()?, &root, range).await;
        result.map_err(|e| self.read_failed(e))
    }

    fn set(&mut self, key: Vec<u8>, value: Bytes) {
        if let State::Open(trx) = &self.state {
            trx.set(&prefixed(&self.root, &key), &value);
        }
    }

    fn clear(&mut self, key: Vec<u8>) {
        if let State::Open(trx) = &self.state {
            trx.clear(&prefixed(&self.root, &key));
        }
    }

    fn clear_range(&mut self, start: Vec<u8>, end: Vec<u8>) {
        if let State::Open(trx) = &self.state {
            if start < end {
                trx.clear_range(&prefixed(&self.root, &start), &prefixed(&self.root, &end));
            }
        }
    }

    fn add(&mut self, key: Vec<u8>, delta: i64) {
        if let State::Open(trx) = &self.state {
            trx.atomic_op(
                &prefixed(&self.root, &key),
                &delta.to_le_bytes(),
                MutationType::Add,
            );
        }
    }

    fn set_versionstamped_key(&mut self, key: Vec<u8>, value: Bytes) {
        self.stamped = true;
        if let State::Open(trx) = &self.state {
            // The trailing offset counts from the start of the key, which
            // now begins with the root.
            let (body, offset) = key.split_at(key.len() - 4);
            let offset = u32::from_le_bytes(offset.try_into().expect("a 4-byte offset"))
                + self.root.len() as u32;
            let mut full = prefixed(&self.root, body);
            full.extend_from_slice(&offset.to_le_bytes());
            trx.atomic_op(&full, &value, MutationType::SetVersionstampedKey);
        }
    }

    async fn commit(&mut self) -> Result<Option<[u8; 10]>> {
        let State::Open(trx) = std::mem::replace(&mut self.state, State::Gone) else {
            return Err(Error::Backend("transaction committed twice".into()));
        };
        let stamp = self.stamped.then(|| trx.get_versionstamp());
        match trx.commit().await {
            Ok(_) => match stamp {
                Some(stamp) => {
                    let stamp = stamp.await.map_err(backend)?;
                    let stamp: [u8; 10] = stamp[..]
                        .try_into()
                        .map_err(|_| Error::Backend("versionstamp is not 10 bytes".into()))?;
                    Ok(Some(stamp))
                }
                None => Ok(None),
            },
            Err(e) => {
                let error = classify(*e, true);
                self.state = State::Failed(e);
                Err(error)
            }
        }
    }

    async fn reset_after(&mut self, _error: &Error) -> Result<()> {
        let trx = match std::mem::replace(&mut self.state, State::Gone) {
            State::Failed(e) => e.on_error().await.map_err(backend)?,
            State::Open(trx) => match self.failure.take() {
                Some(e) => trx.on_error(e).await.map_err(backend)?,
                None => return Err(Error::Backend("no failure to retry".into())),
            },
            State::Gone => return Err(Error::Backend("transaction is gone".into())),
        };
        self.state = State::Open(trx);
        self.stamped = false;
        Ok(())
    }
}
