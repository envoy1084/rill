use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_store::meta::{KeyRange, MetaStore, Mutation, Transaction, TxOutcome};
use rill_store::Result;

use super::plan::{injected_error, Fault, FaultPlan};

/// A metadata store that fails according to a [`FaultPlan`].
pub struct FaultyMetaStore {
    inner: Arc<dyn MetaStore>,
    plan: Arc<FaultPlan>,
}

impl FaultyMetaStore {
    /// Wrap `inner`.
    pub fn new(inner: Arc<dyn MetaStore>, plan: Arc<FaultPlan>) -> Self {
        FaultyMetaStore { inner, plan }
    }
}

#[async_trait]
impl MetaStore for FaultyMetaStore {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        if self.plan.read_fails(("get", key)) {
            return Err(injected_error("meta read"));
        }
        self.inner.get(key).await
    }

    async fn scan(&self, range: KeyRange) -> Result<Vec<(Bytes, Bytes)>> {
        if self.plan.read_fails(("scan", format!("{range:?}"))) {
            return Err(injected_error("meta scan"));
        }
        self.inner.scan(range).await
    }

    async fn counter(&self, key: &[u8]) -> Result<i64> {
        if self.plan.read_fails(("counter", key)) {
            return Err(injected_error("meta counter"));
        }
        self.inner.counter(key).await
    }

    async fn transact(&self, tx: Transaction) -> Result<TxOutcome> {
        match self.plan.write(written_keys(&tx)) {
            Fault::None => self.inner.transact(tx).await,
            Fault::Fail => Err(injected_error("transaction")),
            Fault::LoseAck => {
                self.inner.transact(tx).await?;
                Err(injected_error("transaction committed, reply lost"))
            }
        }
    }
}

/// The keys a transaction writes; values are left out because they can
/// carry random ids.
fn written_keys(tx: &Transaction) -> Vec<&[u8]> {
    tx.mutations
        .iter()
        .map(|m| match m {
            Mutation::Set { key, .. } | Mutation::Delete { key } => key.as_ref(),
            Mutation::AppendLog { prefix, .. } | Mutation::AppendStamped { prefix, .. } => {
                prefix.as_ref()
            }
            Mutation::Add { key, .. } => key.as_ref(),
        })
        .collect()
}
