//! One process's view of the metadata store, which can be cut off: while
//! partitioned, every call it makes fails, as when a server loses its
//! network to the database while the rest of the cluster carries on.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_store::meta::{KeyRange, MetaStore, Transaction, TxOutcome};
use rill_store::Result;

use super::plan::injected_error;

/// A metadata store a process reaches until it is partitioned.
pub struct PartitionedMetaStore {
    inner: Arc<dyn MetaStore>,
    cut: Arc<AtomicBool>,
}

impl PartitionedMetaStore {
    pub fn new(inner: Arc<dyn MetaStore>, cut: Arc<AtomicBool>) -> Self {
        PartitionedMetaStore { inner, cut }
    }

    fn reachable(&self) -> Result<()> {
        if self.cut.load(Ordering::SeqCst) {
            return Err(injected_error("partitioned from the metadata store"));
        }
        Ok(())
    }
}

#[async_trait]
impl MetaStore for PartitionedMetaStore {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.reachable()?;
        self.inner.get(key).await
    }

    async fn scan(&self, range: KeyRange) -> Result<Vec<(Bytes, Bytes)>> {
        self.reachable()?;
        self.inner.scan(range).await
    }

    async fn counter(&self, key: &[u8]) -> Result<i64> {
        self.reachable()?;
        self.inner.counter(key).await
    }

    async fn transact(&self, tx: Transaction) -> Result<TxOutcome> {
        self.reachable()?;
        self.inner.transact(tx).await
    }
}
