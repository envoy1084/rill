//! Backend behaviour against in-memory, local and misbehaving services.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use object_store::memory::InMemory;
use object_store::path::Path;
use object_store::{ObjectStoreExt as _, PutOptions};
use rill_types::{ObjectHash, ObjectKind};
use url::Url;

use super::{ObjectStoreBackend, PutStrategy};
use crate::error::{Error, Result};
use crate::object::ObjectStore;
use crate::seal::{ObjectAt, Sealer, DEFAULT_NAMESPACE};

/// S3 settings come from the environment: a PUT goes to `AWS_ENDPOINT`
/// signed with `AWS_ACCESS_KEY_ID`, not to the instance metadata
/// service.
#[cfg(feature = "s3")]
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
    ) -> futures::stream::BoxStream<'static, object_store::Result<object_store::ObjectMeta>> {
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
