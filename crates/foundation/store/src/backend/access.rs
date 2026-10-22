//! Reading and writing sealed, compressed frames through the service.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{self, StreamExt, TryStreamExt};
use object_store::{ObjectStoreExt as _, PutMode, PutOptions};
use rill_types::{ObjectHash, ObjectKind};

use super::{ObjectStoreBackend, PutStrategy};
use crate::codec;
use crate::error::{Error, Result};
use crate::object::{verify, ObjectStore};
use crate::seal::ObjectAt;

/// How many `has`/`get` requests a batch call keeps in flight.
const BATCH_CONCURRENCY: usize = 16;

impl ObjectStoreBackend {
    async fn fetch(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        let path = self.path_for(kind, hash);
        self.sealer.ready(&self.namespace).await?;
        match self.store.get(&path).await {
            Ok(result) => {
                let stored = result.bytes().await.map_err(Error::backend)?;
                let large = stored.len() > 64 * 1024;
                let (sealer, namespace, hash) =
                    (self.sealer.clone(), self.namespace.clone(), *hash);
                let decode = move || {
                    let at = ObjectAt {
                        namespace: &namespace,
                        kind,
                        hash: &hash,
                    };
                    codec::decompress(&sealer.open(at, stored.to_vec())?)
                };
                // Opening and decompressing are CPU work proportional to the
                // object; keep them off the reactor for anything larger than
                // a tree node.
                let decoded = if large {
                    tokio::task::spawn_blocking(decode)
                        .await
                        .map_err(Error::backend)??
                } else {
                    decode()?
                };
                Ok(Some(Bytes::from(decoded)))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(Error::backend(e)),
        }
    }

    async fn exists(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        let path = self.path_for(kind, hash);
        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(Error::backend(e)),
        }
    }
}

#[async_trait]
impl ObjectStore for ObjectStoreBackend {
    async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        self.fetch(kind, hash).await
    }

    async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        self.exists(kind, hash).await
    }

    async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()> {
        let expected = *hash;
        let (sealer, namespace) = (self.sealer.clone(), self.namespace.clone());
        sealer.ready(&namespace).await?;
        let frame = tokio::task::spawn_blocking(move || {
            verify(kind, &expected, &bytes)?;
            let at = ObjectAt {
                namespace: &namespace,
                kind,
                hash: &expected,
            };
            Ok::<_, Error>(Bytes::from(sealer.seal(at, codec::compress(&bytes))?))
        })
        .await
        .map_err(Error::backend)??;

        let path = self.path_for(kind, hash);
        let mode = match self.strategy {
            PutStrategy::Conditional => PutMode::Create,
            PutStrategy::CheckThenPut => {
                if self.exists(kind, hash).await? {
                    return Ok(());
                }
                PutMode::Overwrite
            }
        };
        match self
            .store
            .put_opts(&path, frame.into(), PutOptions::from(mode))
            .await
        {
            Ok(_) => Ok(()),
            // Content-addressed: whoever got there first wrote the same bytes.
            Err(object_store::Error::AlreadyExists { .. }) => Ok(()),
            Err(e) => Err(Error::backend(e)),
        }
    }

    fn namespaced(&self, namespace: &str) -> Arc<dyn ObjectStore> {
        Arc::new(ObjectStoreBackend {
            prefix: self.base.clone().join(namespace),
            namespace: Arc::from(namespace),
            ..self.clone()
        })
    }

    async fn has_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<bool>> {
        stream::iter(items.iter().copied())
            .map(|(kind, hash)| async move { self.exists(kind, &hash).await })
            .buffered(BATCH_CONCURRENCY)
            .try_collect()
            .await
    }

    async fn get_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<Option<Bytes>>> {
        stream::iter(items.iter().copied())
            .map(|(kind, hash)| async move { self.fetch(kind, &hash).await })
            .buffered(BATCH_CONCURRENCY)
            .try_collect()
            .await
    }
}
