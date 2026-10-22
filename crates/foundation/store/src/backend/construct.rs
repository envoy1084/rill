//! Building a backend from a URL or a local directory.

use std::sync::Arc;

use object_store::path::Path;
use url::Url;

use super::{ObjectStoreBackend, PutStrategy};
use crate::error::{Error, Result};
use crate::seal::{Plaintext, DEFAULT_NAMESPACE};

impl ObjectStoreBackend {
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
            #[cfg(feature = "s3")]
            "s3" | "s3a" => {
                let store = object_store::aws::AmazonS3Builder::from_env()
                    .with_url(url.as_str())
                    .build()
                    .map_err(Error::backend)?;
                (Box::new(store), Path::from(url.path()))
            }
            #[cfg(not(feature = "s3"))]
            "s3" | "s3a" => {
                return Err(Error::Backend(
                    "this build has no S3 support (feature s3)".into(),
                ))
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

    /// Objects in files under `dir` (created if missing), each write
    /// flushed to disk with its directory before it is acknowledged.
    pub async fn local(dir: impl AsRef<std::path::Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        let store = tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&dir)?;
            object_store::local::LocalFileSystem::new_with_prefix(&dir)
                .map(|fs| fs.with_fsync(true))
                .map_err(Error::backend)
        })
        .await
        .map_err(Error::backend)??;
        Ok(ObjectStoreBackend::new(Arc::new(store), ""))
    }
}
