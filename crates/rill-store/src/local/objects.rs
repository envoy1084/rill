//! Content-addressed object store on a local filesystem.
//!
//! Layout: `<root>/<namespace>/<kind>/<hh>/<hash-hex>` where `hh` is the
//! first two hex characters of the hash, keeping directories small. Each
//! file is a [`codec`](crate::codec) frame, passed through the store's
//! [`Sealer`]. Writes go to a temp file in the final
//! directory, are fsynced, then renamed into place, so a reader never sees a
//! partial object and a crash leaves at worst a stray `.tmp-*` file.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_types::{ObjectHash, ObjectKind};

use crate::codec;
use crate::error::{Error, Result};
use crate::object::{verify, ObjectStore};
use crate::seal::{Plaintext, Sealer, DEFAULT_NAMESPACE};

/// Filesystem-backed [`ObjectStore`].
#[derive(Debug, Clone)]
pub struct LocalObjectStore {
    inner: Arc<Inner>,
}

/// How a [`LocalObjectStore`] lays out and encodes objects.
#[derive(Debug, Clone)]
pub struct LocalOptions {
    /// zstd level for new objects.
    pub level: i32,
    pub namespace: String,
    pub sealer: Arc<dyn Sealer>,
}

impl Default for LocalOptions {
    fn default() -> Self {
        LocalOptions {
            level: codec::DEFAULT_LEVEL,
            namespace: DEFAULT_NAMESPACE.to_string(),
            sealer: Arc::new(Plaintext),
        }
    }
}

#[derive(Debug)]
struct Inner {
    /// The directory holding every namespace.
    base: PathBuf,
    root: PathBuf,
    level: i32,
    sealer: Arc<dyn Sealer>,
    /// Distinguishes temp files of concurrent writers for the same object.
    temp_counter: AtomicU64,
}

impl LocalObjectStore {
    /// Open (creating if needed) a store rooted at `root`.
    pub async fn open(root: impl Into<PathBuf>) -> Result<Self> {
        Self::open_with(root, LocalOptions::default()).await
    }

    /// Open a store that compresses new objects with the given zstd level.
    pub async fn open_with_level(root: impl Into<PathBuf>, level: i32) -> Result<Self> {
        Self::open_with(
            root,
            LocalOptions {
                level,
                ..LocalOptions::default()
            },
        )
        .await
    }

    /// Open a store with explicit options.
    pub async fn open_with(root: impl Into<PathBuf>, options: LocalOptions) -> Result<Self> {
        let base = root.into();
        let root = base.join(&options.namespace);
        let dir = root.clone();
        tokio::task::spawn_blocking(move || fs::create_dir_all(&dir))
            .await
            .map_err(Error::backend)??;
        Ok(LocalObjectStore {
            inner: Arc::new(Inner {
                base,
                root,
                level: options.level,
                sealer: options.sealer,
                temp_counter: AtomicU64::new(0),
            }),
        })
    }

    /// Root directory of the store.
    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    async fn blocking<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Inner) -> Result<T> + Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || f(&inner))
            .await
            .map_err(Error::backend)?
    }
}

impl Inner {
    fn path_for(&self, kind: ObjectKind, hash: &ObjectHash) -> PathBuf {
        let hex = hash.to_hex();
        self.root.join(kind.as_str()).join(&hex[..2]).join(hex)
    }

    fn read(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        match fs::read(self.path_for(kind, hash)) {
            Ok(stored) => {
                let frame = self.sealer.open(stored)?;
                Ok(Some(Bytes::from(codec::decompress(&frame)?)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn exists(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        Ok(self.path_for(kind, hash).try_exists()?)
    }

    fn write(&self, kind: ObjectKind, hash: &ObjectHash, bytes: &[u8]) -> Result<()> {
        let path = self.path_for(kind, hash);
        if path.try_exists()? {
            return Ok(());
        }
        // The directory existing is not enough for durability of the rename:
        // the directory entry itself must be fsynced after the rename.
        let dir = path.parent().ok_or_else(|| {
            Error::Backend(format!("object path {} has no parent", path.display()))
        })?;
        fs::create_dir_all(dir)?;

        let frame = self
            .sealer
            .seal(codec::compress_with_level(bytes, self.level))?;
        let tmp = dir.join(format!(
            ".tmp-{}-{}-{}",
            std::process::id(),
            self.temp_counter.fetch_add(1, Ordering::Relaxed),
            hash.short()
        ));
        let result = write_temp(&tmp, &frame).and_then(|()| {
            if path.try_exists()? {
                // Another writer won the race; the content is identical by
                // construction, so keep theirs.
                fs::remove_file(&tmp)?;
                return Ok(());
            }
            fs::rename(&tmp, &path)?;
            fs::File::open(dir)?.sync_all()?;
            Ok(())
        });
        if result.is_err() {
            // Best effort: a leftover temp file is harmless but untidy.
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

fn write_temp(tmp: &Path, frame: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(tmp)?;
    file.write_all(frame)?;
    file.sync_all()?;
    Ok(())
}

#[async_trait]
impl ObjectStore for LocalObjectStore {
    async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        let hash = *hash;
        self.blocking(move |inner| inner.read(kind, &hash)).await
    }

    async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        let hash = *hash;
        self.blocking(move |inner| inner.exists(kind, &hash)).await
    }

    async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()> {
        let hash = *hash;
        self.blocking(move |inner| {
            verify(kind, &hash, &bytes)?;
            inner.write(kind, &hash, &bytes)
        })
        .await
    }

    fn namespaced(&self, namespace: &str) -> Arc<dyn ObjectStore> {
        Arc::new(LocalObjectStore {
            inner: Arc::new(Inner {
                base: self.inner.base.clone(),
                root: self.inner.base.join(namespace),
                level: self.inner.level,
                sealer: self.inner.sealer.clone(),
                temp_counter: AtomicU64::new(0),
            }),
        })
    }

    async fn has_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<bool>> {
        let items = items.to_vec();
        self.blocking(move |inner| {
            items
                .iter()
                .map(|(kind, hash)| inner.exists(*kind, hash))
                .collect()
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn layout_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalObjectStore::open(dir.path()).await.unwrap();
        let bytes = Bytes::from(vec![b'z'; 1000]);
        let hash = ObjectHash::digest(ObjectKind::Tree, &bytes);
        store
            .put(ObjectKind::Tree, &hash, bytes.clone())
            .await
            .unwrap();

        let hex = hash.to_hex();
        let path = dir
            .path()
            .join(DEFAULT_NAMESPACE)
            .join("tree")
            .join(&hex[..2])
            .join(&hex);
        let on_disk = fs::read(&path).unwrap();
        assert_eq!(&on_disk[..4], b"RILZ");
        assert!(on_disk.len() < bytes.len());

        assert_eq!(
            store.get(ObjectKind::Tree, &hash).await.unwrap(),
            Some(bytes)
        );
        assert!(store.has(ObjectKind::Tree, &hash).await.unwrap());
        assert!(!store.has(ObjectKind::Blob, &hash).await.unwrap());
        assert_eq!(
            store
                .has_many(&[(ObjectKind::Tree, hash), (ObjectKind::Blob, hash)])
                .await
                .unwrap(),
            vec![true, false]
        );
        assert!(!fs::read_dir(path.parent().unwrap()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".tmp")));
    }

    #[tokio::test]
    async fn put_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalObjectStore::open(dir.path()).await.unwrap();
        let bytes = Bytes::from_static(b"same");
        let hash = ObjectHash::digest(ObjectKind::Blob, &bytes);
        store
            .put(ObjectKind::Blob, &hash, bytes.clone())
            .await
            .unwrap();
        let path = dir
            .path()
            .join(DEFAULT_NAMESPACE)
            .join("blob")
            .join(&hash.to_hex()[..2])
            .join(hash.to_hex());
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        store.put(ObjectKind::Blob, &hash, bytes).await.unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    }

    #[tokio::test]
    async fn rejects_hash_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalObjectStore::open(dir.path()).await.unwrap();
        let wrong = ObjectHash::digest(ObjectKind::Blob, b"other");
        let err = store
            .put(ObjectKind::Blob, &wrong, Bytes::from_static(b"data"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::HashMismatch { .. }));
        assert!(!store.has(ObjectKind::Blob, &wrong).await.unwrap());
    }
}
