//! Counting and timing metadata transactions.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use bytes::Bytes;
use rill_store::{Error, KeyRange, MetaStore, Result, Transaction, TxOutcome};

use super::metrics::{Metrics, OutcomeLabels};

/// A metadata store that records every transaction in [`Metrics`].
pub struct InstrumentedMetaStore {
    inner: Arc<dyn MetaStore>,
    metrics: Arc<Metrics>,
}

impl InstrumentedMetaStore {
    pub fn new(inner: Arc<dyn MetaStore>, metrics: Arc<Metrics>) -> Self {
        InstrumentedMetaStore { inner, metrics }
    }
}

#[async_trait]
impl MetaStore for InstrumentedMetaStore {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.inner.get(key).await
    }

    async fn get_many(&self, keys: &[Bytes]) -> Result<Vec<Option<Bytes>>> {
        self.inner.get_many(keys).await
    }

    async fn scan(&self, range: KeyRange) -> Result<Vec<(Bytes, Bytes)>> {
        self.inner.scan(range).await
    }

    async fn transact(&self, tx: Transaction) -> Result<TxOutcome> {
        let started = Instant::now();
        let outcome = self.inner.transact(tx).await;
        self.metrics
            .meta_transaction_duration
            .observe(started.elapsed().as_secs_f64());
        let outcome_label = match &outcome {
            Ok(_) => "committed",
            Err(Error::Conflict(_)) => "conflict",
            Err(_) => "error",
        };
        self.metrics
            .meta_transactions
            .get_or_create(&OutcomeLabels {
                outcome: outcome_label,
            })
            .inc();
        outcome
    }

    async fn counter(&self, key: &[u8]) -> Result<i64> {
        self.inner.counter(key).await
    }

    async fn wait_counter(
        &self,
        key: &[u8],
        seen: i64,
        timeout: std::time::Duration,
    ) -> Result<i64> {
        self.inner.wait_counter(key, seen, timeout).await
    }
}
