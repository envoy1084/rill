use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_store::object::ObjectStore;
use rill_store::Result;
use rill_types::{ObjectHash, ObjectKind};

use super::plan::{injected_error, Fault, FaultPlan};

/// An object store that fails according to a [`FaultPlan`].
pub struct FaultyObjectStore {
    inner: Arc<dyn ObjectStore>,
    plan: Arc<FaultPlan>,
}

impl FaultyObjectStore {
    /// Wrap `inner`.
    pub fn new(inner: Arc<dyn ObjectStore>, plan: Arc<FaultPlan>) -> Self {
        FaultyObjectStore { inner, plan }
    }
}

#[async_trait]
impl ObjectStore for FaultyObjectStore {
    async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        if self.plan.read_fails(("get", kind as u8, hash.as_bytes())) {
            return Err(injected_error("object read"));
        }
        let bytes = self.inner.get(kind, hash).await?;
        match bytes {
            Some(bytes)
                if self
                    .plan
                    .read_corrupts(("get", kind as u8, hash.as_bytes())) =>
            {
                let mut flipped = bytes.to_vec();
                if let Some(first) = flipped.first_mut() {
                    *first ^= 0xFF;
                }
                Ok(Some(Bytes::from(flipped)))
            }
            other => Ok(other),
        }
    }

    async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        if self.plan.read_fails(("has", kind as u8, hash.as_bytes())) {
            return Err(injected_error("object has"));
        }
        self.inner.has(kind, hash).await
    }

    fn namespaced(&self, namespace: &str) -> Arc<dyn ObjectStore> {
        Arc::new(FaultyObjectStore::new(
            self.inner.namespaced(namespace),
            self.plan.clone(),
        ))
    }

    async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()> {
        match self.plan.write(("put", kind as u8, hash.as_bytes())) {
            Fault::None => self.inner.put(kind, hash, bytes).await,
            Fault::Fail => Err(injected_error("object put")),
            Fault::LoseAck => {
                self.inner.put(kind, hash, bytes).await?;
                Err(injected_error("object stored, reply lost"))
            }
        }
    }
}
