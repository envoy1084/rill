//! The engine of single-process backends: writers take turns, and a
//! transaction's writes are applied as one atomic batch on commit.
//!
//! Holding the writer turn from `begin` to `commit` makes every
//! transaction serializable: nothing else writes while it reads.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::{Mutex, OwnedMutexGuard};

use super::engine::{fill_stamp, Engine, EngineTxn, RawRange};
use crate::error::{Error, Result};

/// One buffered write.
#[derive(Debug, Clone)]
pub enum Op {
    Set(Bytes, Bytes),
    Clear(Bytes),
    ClearRange(Bytes, Bytes),
    Add(Bytes, i64),
    SetStamped(Bytes, Bytes),
}

/// Storage that reads committed state and applies batches atomically.
#[async_trait]
pub trait Batch: Send + Sync + 'static {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>>;

    /// One call reads one consistent snapshot.
    async fn scan(&self, range: &RawRange) -> Result<Vec<(Bytes, Bytes)>>;

    /// Apply `ops` in order, all or nothing. When `stamp` is set, assign the
    /// next commit stamp (strictly greater than every earlier one, across
    /// restarts), fill it into `SetStamped` keys and return it.
    async fn apply(&self, ops: Vec<Op>, stamp: bool) -> Result<Option<[u8; 10]>>;
}

/// An [`Engine`] over a [`Batch`] backend.
pub struct Buffered<B> {
    backend: Arc<B>,
    writer: Arc<Mutex<()>>,
}

impl<B: Batch> Buffered<B> {
    /// Wrap `backend`.
    pub fn new(backend: B) -> Self {
        Buffered {
            backend: Arc::new(backend),
            writer: Arc::new(Mutex::new(())),
        }
    }

    /// The wrapped backend.
    pub fn backend(&self) -> &B {
        &self.backend
    }
}

#[async_trait]
impl<B: Batch> Engine for Buffered<B> {
    type Txn = BufferedTxn<B>;

    async fn begin(&self) -> Result<Self::Txn> {
        let turn = Arc::clone(&self.writer).lock_owned().await;
        Ok(BufferedTxn {
            backend: Arc::clone(&self.backend),
            _turn: turn,
            ops: Vec::new(),
            stamped: false,
        })
    }

    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.backend.get(key).await
    }

    async fn scan(&self, range: &RawRange) -> Result<Vec<(Bytes, Bytes)>> {
        self.backend.scan(range).await
    }
}

/// A transaction holding the writer turn.
pub struct BufferedTxn<B> {
    backend: Arc<B>,
    _turn: OwnedMutexGuard<()>,
    ops: Vec<Op>,
    stamped: bool,
}

#[async_trait]
impl<B: Batch> EngineTxn for BufferedTxn<B> {
    async fn get(&mut self, key: &[u8]) -> Result<Option<Bytes>> {
        self.backend.get(key).await
    }

    async fn scan(&mut self, range: &RawRange) -> Result<Vec<(Bytes, Bytes)>> {
        self.backend.scan(range).await
    }

    fn set(&mut self, key: &[u8], value: &[u8]) {
        self.ops.push(Op::Set(
            Bytes::copy_from_slice(key),
            Bytes::copy_from_slice(value),
        ));
    }

    fn clear(&mut self, key: &[u8]) {
        self.ops.push(Op::Clear(Bytes::copy_from_slice(key)));
    }

    fn clear_range(&mut self, start: &[u8], end: &[u8]) {
        self.ops.push(Op::ClearRange(
            Bytes::copy_from_slice(start),
            Bytes::copy_from_slice(end),
        ));
    }

    fn add(&mut self, key: &[u8], delta: i64) {
        self.ops.push(Op::Add(Bytes::copy_from_slice(key), delta));
    }

    fn set_versionstamped_key(&mut self, key: &[u8], value: &[u8]) {
        self.stamped = true;
        self.ops.push(Op::SetStamped(
            Bytes::copy_from_slice(key),
            Bytes::copy_from_slice(value),
        ));
    }

    async fn commit(&mut self) -> Result<Option<[u8; 10]>> {
        let ops = std::mem::take(&mut self.ops);
        if ops.is_empty() {
            return Ok(None);
        }
        self.backend.apply(ops, self.stamped).await
    }

    async fn reset_after(&mut self, error: &Error) -> Result<()> {
        Err(Error::Backend(format!("unexpected retry: {error}")))
    }
}

/// The stamp of commit number `n`.
pub fn stamp_of(n: u64) -> [u8; 10] {
    let mut stamp = [0u8; 10];
    stamp[..8].copy_from_slice(&n.to_be_bytes());
    stamp
}

/// Fill `stamp` into every stamped key, so applying the batch cannot fail
/// part-way.
pub fn resolve_stamps(ops: Vec<Op>, stamp: Option<&[u8; 10]>) -> Result<Vec<Op>> {
    ops.into_iter()
        .map(|op| match op {
            Op::SetStamped(k, v) => {
                let stamp =
                    stamp.ok_or_else(|| Error::Backend("stamped write without a stamp".into()))?;
                Ok(Op::SetStamped(Bytes::from(fill_stamp(&k, stamp)?), v))
            }
            other => Ok(other),
        })
        .collect()
}
