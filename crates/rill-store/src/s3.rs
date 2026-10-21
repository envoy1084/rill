//! Object store over the [`object_store`] crate: S3 and S3-compatible
//! services, plus the in-memory and local-filesystem stores it ships, which
//! make this backend testable without network access.
//!
//! Keys are `<prefix>/<namespace>/<kind>/<hh>/<hash-hex>`, the same shape as
//! the local store, and values are [`codec`](crate::codec) frames passed
//! through the backend's [`Sealer`]. Writes use conditional put
//! (`If-None-Match: *`) so an object is never overwritten. Some
//! S3-compatible services do not support it; [`ObjectStoreBackend::probe`]
//! detects that at startup, and when allowed the backend falls back to
//! checking for the object before an unconditional put, which is safe
//! because content-addressed writes of one key are always identical.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{self, StreamExt, TryStreamExt};
use object_store::path::Path;
use object_store::{ObjectStoreExt as _, PutMode, PutOptions};
use rill_types::{ObjectHash, ObjectKind};
use url::Url;

use crate::codec;
use crate::error::{Error, Result};
use crate::object::{verify, ObjectStore};
use crate::seal::{ObjectAt, Plaintext, Sealer, DEFAULT_NAMESPACE};

/// How many `has`/`get` requests a batch call keeps in flight.
const BATCH_CONCURRENCY: usize = 16;

/// How writes avoid overwriting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutStrategy {
    /// `PutMode::Create`, enforced by the service.
    Conditional,
    /// A `HEAD` before an unconditional put, for services without
    /// conditional put.
    CheckThenPut,
}

/// [`ObjectStore`] over any [`object_store::ObjectStore`].
#[derive(Debug, Clone)]
pub struct ObjectStoreBackend {
    store: Arc<dyn object_store::ObjectStore>,
    /// The prefix holding every namespace.
    base: Path,
    namespace: Arc<str>,
    prefix: Path,
    sealer: Arc<dyn Sealer>,
    strategy: PutStrategy,
}

impl ObjectStoreBackend {
    /// Wrap `store`, placing every key under `prefix` (may be empty) and the
    /// default namespace.
    pub fn new(store: Arc<dyn object_store::ObjectStore>, prefix: impl AsRef<str>) -> Self {
        let base = Path::from(prefix.as_ref());
        ObjectStoreBackend {
            store,
            prefix: base.clone().join(DEFAULT_NAMESPACE),
            namespace: Arc::from(DEFAULT_NAMESPACE),
            base,
            sealer: Arc::new(Plaintext),
            strategy: PutStrategy::Conditional,
        }
    }

    /// Seal frames with `sealer`.
    pub fn with_sealer(mut self, sealer: Arc<dyn Sealer>) -> Self {
        self.sealer = sealer;
        self
    }

    /// The write strategy in use.
    pub fn strategy(&self) -> PutStrategy {
        self.strategy
    }

    /// Check whether the service honours conditional put by creating a probe
    /// object twice. Without it the backend refuses to start unless
    /// `allow_unconditional` is set, in which case it falls back to
    /// [`PutStrategy::CheckThenPut`].
    pub async fn probe(mut self, allow_unconditional: bool) -> Result<Self> {
        let path = self.prefix.clone().join(".rill-probe");
        let create = || {
            self.store.put_opts(
                &path,
                Bytes::from_static(b"probe").into(),
                PutOptions::from(PutMode::Create),
            )
        };
        // Supported means the second create of the same key is refused. An
        // unsupported mode errors either way or lets the overwrite through.
        let supported = match create().await {
            Err(e) if is_unsupported(&e) => false,
            Err(object_store::Error::AlreadyExists { .. }) | Ok(_) => match create().await {
                Err(object_store::Error::AlreadyExists { .. }) => true,
                Ok(_) => false,
                Err(e) if is_unsupported(&e) => false,
                Err(e) => return Err(Error::backend(e)),
            },
            Err(e) => return Err(Error::backend(e)),
        };
        if supported {
            self.strategy = PutStrategy::Conditional;
            return Ok(self);
        }
        if !allow_unconditional {
            return Err(Error::Backend(
                "object store does not support conditional put; set \
                 allow_unconditional_put to fall back to check-then-put"
                    .into(),
            ));
        }
        tracing::warn!("object store lacks conditional put; using check-then-put");
        self.strategy = PutStrategy::CheckThenPut;
        Ok(self)
    }

    /// Build a backend from a URL such as `s3://bucket/path`, `file:///dir`
    /// or `memory:///`. The URL's own path and `prefix` are joined. For S3,
    /// credentials, region and endpoint come from the `AWS_*` environment
    /// variables (`AWS_ACCESS_KEY_ID`, `AWS_ENDPOINT`, `AWS_ALLOW_HTTP`, ...).
    pub fn from_url(url: &str, prefix: impl AsRef<str>) -> Result<Self> {
        let url =
            Url::parse(url).map_err(|e| Error::Backend(format!("bad object store url: {e}")))?;
        let (store, url_path): (Box<dyn object_store::ObjectStore>, Path) = match url.scheme() {
            // `parse_url` ignores the environment; S3 needs it for
            // credentials and non-AWS endpoints.
            "s3" | "s3a" => {
                let store = object_store::aws::AmazonS3Builder::from_env()
                    .with_url(url.as_str())
                    .build()
                    .map_err(Error::backend)?;
                (Box::new(store), Path::from(url.path()))
            }
            _ => object_store::parse_url(&url).map_err(Error::backend)?,
        };
        let mut full = url_path;
        for part in Path::from(prefix.as_ref()).parts() {
            full = full.join(part);
        }
        Ok(ObjectStoreBackend {
            store: Arc::from(store),
            prefix: full.clone().join(DEFAULT_NAMESPACE),
            namespace: Arc::from(DEFAULT_NAMESPACE),
            base: full,
            sealer: Arc::new(Plaintext),
            strategy: PutStrategy::Conditional,
        })
    }

    /// Key prefix every object lives under.
    pub fn prefix(&self) -> &Path {
        &self.prefix
    }

    fn path_for(&self, kind: ObjectKind, hash: &ObjectHash) -> Path {
        let hex = hash.to_hex();
        self.prefix
            .clone()
            .join(kind.as_str())
            .join(&hex[..2])
            .join(hex.as_str())
    }

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

/// Errors a service gives for a conditional put it does not implement.
fn is_unsupported(e: &object_store::Error) -> bool {
    matches!(
        e,
        object_store::Error::NotImplemented { .. } | object_store::Error::NotSupported { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    /// S3 settings come from the environment: a PUT goes to `AWS_ENDPOINT`
    /// signed with `AWS_ACCESS_KEY_ID`, not to the instance metadata
    /// service.
    #[tokio::test]
    async fn s3_urls_take_credentials_and_endpoint_from_the_environment() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let seen = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0u8; 8192];
            let n = socket.read(&mut request).await.unwrap();
            let text = String::from_utf8_lossy(&request[..n]).to_string();
            let reply = "HTTP/1.1 200 OK\r\nETag: \"e\"\r\nContent-Length: 0\r\n\r\n";
            socket.write_all(reply.as_bytes()).await.unwrap();
            text
        });
        // SAFETY of the environment: no other test reads these variables.
        std::env::set_var("AWS_ENDPOINT", &endpoint);
        std::env::set_var("AWS_ALLOW_HTTP", "true");
        std::env::set_var("AWS_ACCESS_KEY_ID", "rill-test-key");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "rill-test-secret");
        std::env::set_var("AWS_REGION", "us-east-1");
        let backend = ObjectStoreBackend::from_url("s3://bucket", "staging").unwrap();
        let _ = backend
            .store
            .put(&Path::from("probe"), Bytes::from_static(b"x").into())
            .await;
        let request = tokio::time::timeout(std::time::Duration::from_secs(10), seen)
            .await
            .expect("the configured endpoint was called")
            .unwrap();
        assert!(request.starts_with("PUT /bucket/probe"), "{request}");
        assert!(
            request.contains("rill-test-key"),
            "signed with the environment's key"
        );
    }

    fn backend() -> ObjectStoreBackend {
        ObjectStoreBackend::new(Arc::new(InMemory::new()), "rill/objects")
    }

    #[tokio::test]
    async fn keys_are_prefixed_and_sharded() {
        let b = backend();
        let hash = ObjectHash::digest(ObjectKind::Blob, b"x");
        let hex = hash.to_hex();
        assert_eq!(
            b.path_for(ObjectKind::Blob, &hash).as_ref(),
            format!("rill/objects/default/blob/{}/{}", &hex[..2], hex)
        );
    }

    #[tokio::test]
    async fn roundtrip_and_batches() {
        let b = backend();
        let bytes = Bytes::from(vec![7u8; 2048]);
        let hash = ObjectHash::digest(ObjectKind::Chunk, &bytes);
        assert!(!b.has(ObjectKind::Chunk, &hash).await.unwrap());
        b.put(ObjectKind::Chunk, &hash, bytes.clone())
            .await
            .unwrap();
        // Second put of the same object hits AlreadyExists and succeeds.
        b.put(ObjectKind::Chunk, &hash, bytes.clone())
            .await
            .unwrap();
        assert_eq!(
            b.get(ObjectKind::Chunk, &hash).await.unwrap(),
            Some(bytes.clone())
        );
        let other = ObjectHash::digest(ObjectKind::Chunk, b"absent");
        assert_eq!(
            b.has_many(&[(ObjectKind::Chunk, hash), (ObjectKind::Chunk, other)])
                .await
                .unwrap(),
            vec![true, false]
        );
        assert_eq!(
            b.get_many(&[(ObjectKind::Chunk, other), (ObjectKind::Chunk, hash)])
                .await
                .unwrap(),
            vec![None, Some(bytes)]
        );
    }

    #[tokio::test]
    async fn rejects_hash_mismatch() {
        let b = backend();
        let wrong = ObjectHash::digest(ObjectKind::Blob, b"other");
        let err = b
            .put(ObjectKind::Blob, &wrong, Bytes::from_static(b"data"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::HashMismatch { .. }));
        assert!(!b.has(ObjectKind::Blob, &wrong).await.unwrap());
    }

    #[tokio::test]
    async fn from_url_memory_and_file() {
        let mem = ObjectStoreBackend::from_url("memory:///", "p/q").unwrap();
        assert_eq!(mem.prefix().as_ref(), "p/q/default");
        let bytes = Bytes::from_static(b"hello");
        let hash = ObjectHash::digest(ObjectKind::Blob, &bytes);
        mem.put(ObjectKind::Blob, &hash, bytes.clone())
            .await
            .unwrap();
        assert_eq!(
            mem.get(ObjectKind::Blob, &hash).await.unwrap(),
            Some(bytes.clone())
        );

        let dir = tempfile::tempdir().unwrap();
        let url = Url::from_directory_path(dir.path()).unwrap();
        let fs = ObjectStoreBackend::from_url(url.as_str(), "objs").unwrap();
        fs.put(ObjectKind::Blob, &hash, bytes.clone())
            .await
            .unwrap();
        assert_eq!(fs.get(ObjectKind::Blob, &hash).await.unwrap(), Some(bytes));
        let hex = hash.to_hex();
        assert!(dir
            .path()
            .join("objs")
            .join(DEFAULT_NAMESPACE)
            .join("blob")
            .join(&hex[..2])
            .join(&hex)
            .is_file());
    }

    #[test]
    fn from_url_rejects_garbage() {
        assert!(ObjectStoreBackend::from_url("not a url", "").is_err());
    }

    /// An S3-compatible service that ignores `If-None-Match`, as some do:
    /// every put overwrites.
    #[derive(Debug, Default)]
    struct IgnoresConditional(InMemory);

    impl std::fmt::Display for IgnoresConditional {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("ignores-conditional")
        }
    }

    #[async_trait]
    impl object_store::ObjectStore for IgnoresConditional {
        async fn put_opts(
            &self,
            location: &Path,
            payload: object_store::PutPayload,
            _opts: PutOptions,
        ) -> object_store::Result<object_store::PutResult> {
            self.0
                .put_opts(location, payload, PutOptions::default())
                .await
        }
        async fn put_multipart_opts(
            &self,
            location: &Path,
            opts: object_store::PutMultipartOptions,
        ) -> object_store::Result<Box<dyn object_store::MultipartUpload>> {
            self.0.put_multipart_opts(location, opts).await
        }
        async fn get_opts(
            &self,
            location: &Path,
            options: object_store::GetOptions,
        ) -> object_store::Result<object_store::GetResult> {
            self.0.get_opts(location, options).await
        }
        fn delete_stream(
            &self,
            locations: futures::stream::BoxStream<'static, object_store::Result<Path>>,
        ) -> futures::stream::BoxStream<'static, object_store::Result<Path>> {
            self.0.delete_stream(locations)
        }
        fn list(
            &self,
            prefix: Option<&Path>,
        ) -> futures::stream::BoxStream<'static, object_store::Result<object_store::ObjectMeta>>
        {
            self.0.list(prefix)
        }
        async fn list_with_delimiter(
            &self,
            prefix: Option<&Path>,
        ) -> object_store::Result<object_store::ListResult> {
            self.0.list_with_delimiter(prefix).await
        }
        async fn copy_opts(
            &self,
            from: &Path,
            to: &Path,
            options: object_store::CopyOptions,
        ) -> object_store::Result<()> {
            self.0.copy_opts(from, to, options).await
        }
    }

    #[tokio::test]
    async fn probe_detects_missing_conditional_put() {
        let good = backend().probe(false).await.unwrap();
        assert_eq!(good.strategy(), PutStrategy::Conditional);

        let store = Arc::new(IgnoresConditional::default());
        assert!(ObjectStoreBackend::new(store.clone(), "p")
            .probe(false)
            .await
            .is_err());
        let fallback = ObjectStoreBackend::new(store, "p")
            .probe(true)
            .await
            .unwrap();
        assert_eq!(fallback.strategy(), PutStrategy::CheckThenPut);
        let bytes = Bytes::from_static(b"content");
        let hash = ObjectHash::digest(ObjectKind::Blob, &bytes);
        fallback
            .put(ObjectKind::Blob, &hash, bytes.clone())
            .await
            .unwrap();
        fallback
            .put(ObjectKind::Blob, &hash, bytes.clone())
            .await
            .unwrap();
        assert_eq!(
            fallback.get(ObjectKind::Blob, &hash).await.unwrap(),
            Some(bytes)
        );
    }

    /// Reverses bytes: enough to tell sealed from plain on the backend.
    #[derive(Debug)]
    struct Reverse;

    #[async_trait::async_trait]
    impl Sealer for Reverse {
        fn seal(&self, _: ObjectAt<'_>, mut frame: Vec<u8>) -> Result<Vec<u8>> {
            frame.reverse();
            Ok(frame)
        }
        fn open(&self, _: ObjectAt<'_>, mut stored: Vec<u8>) -> Result<Vec<u8>> {
            stored.reverse();
            Ok(stored)
        }
    }

    #[tokio::test]
    async fn frames_pass_through_the_sealer() {
        let raw = Arc::new(InMemory::new());
        let sealed = ObjectStoreBackend::new(raw.clone(), "s").with_sealer(Arc::new(Reverse));
        let bytes = Bytes::from_static(b"secret source code, long enough to frame");
        let hash = ObjectHash::digest(ObjectKind::Blob, &bytes);
        sealed
            .put(ObjectKind::Blob, &hash, bytes.clone())
            .await
            .unwrap();
        assert_eq!(
            sealed.get(ObjectKind::Blob, &hash).await.unwrap(),
            Some(bytes)
        );
        let plain = ObjectStoreBackend::new(raw, "s");
        assert!(plain.get(ObjectKind::Blob, &hash).await.is_err());
    }
}
