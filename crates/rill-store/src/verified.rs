//! Hash checks on read.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_types::{ObjectHash, ObjectKind};

use crate::error::{Error, Result};
use crate::object::{verify, ObjectStore};

/// Re-hashes every object read from `inner`. Puts are already verified by
/// each backend; reads are not, and a flipped bit on disk or a truncated
/// object in a bucket would otherwise be served (and cached forever, since
/// object URLs are immutable). [`Stores::new`](crate::Stores::new) wraps
/// every backend in this.
pub struct VerifiedObjectStore {
    inner: Arc<dyn ObjectStore>,
}

impl VerifiedObjectStore {
    /// Verify reads from `inner`.
    pub fn new(inner: Arc<dyn ObjectStore>) -> Self {
        VerifiedObjectStore { inner }
    }

    fn check(kind: ObjectKind, hash: &ObjectHash, bytes: Option<Bytes>) -> Result<Option<Bytes>> {
        if let Some(bytes) = &bytes {
            if verify(kind, hash, bytes).is_err() {
                tracing::error!(%kind, %hash, "object failed its hash check on read");
                return Err(Error::Corrupt { kind, hash: *hash });
            }
        }
        Ok(bytes)
    }
}

#[async_trait]
impl ObjectStore for VerifiedObjectStore {
    async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        Self::check(kind, hash, self.inner.get(kind, hash).await?)
    }

    async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        self.inner.has(kind, hash).await
    }

    async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()> {
        self.inner.put(kind, hash, bytes).await
    }

    fn namespaced(&self, namespace: &str) -> Arc<dyn ObjectStore> {
        Arc::new(VerifiedObjectStore::new(self.inner.namespaced(namespace)))
    }

    fn refuses(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<()> {
        self.inner.refuses(kind, hash)
    }

    async fn has_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<bool>> {
        self.inner.has_many(items).await
    }

    async fn get_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<Option<Bytes>>> {
        let found = self.inner.get_many(items).await?;
        items
            .iter()
            .zip(found)
            .map(|((kind, hash), bytes)| Self::check(*kind, hash, bytes))
            .collect()
    }
}
