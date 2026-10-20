//! Local on-disk backends: objects as files, metadata in a redb database.
//!
//! This is the single-node deployment: everything under one directory,
//! no services to run. [`LocalStores::open`] wires both halves together.

mod meta;
mod objects;

use std::path::Path;
use std::sync::Arc;

pub use meta::RedbMetaStore;
pub use objects::{LocalObjectStore, LocalOptions};

use crate::error::Result;
use crate::repo::Stores;

/// Opens the pair of local backends under one directory.
pub struct LocalStores;

impl LocalStores {
    /// Subdirectory holding the object files.
    pub const OBJECTS_DIR: &'static str = "objects";
    /// File name of the redb metadata database.
    pub const META_FILE: &'static str = "meta.redb";

    /// Open (creating as needed) `<dir>/objects` and `<dir>/meta.redb`.
    pub async fn open(dir: impl AsRef<Path>) -> Result<Stores> {
        let dir = dir.as_ref();
        let objects = LocalObjectStore::open(dir.join(Self::OBJECTS_DIR)).await?;
        let meta = RedbMetaStore::open(dir.join(Self::META_FILE)).await?;
        Ok(Stores::new(Arc::new(objects), Arc::new(meta)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_creates_layout() {
        let dir = tempfile::tempdir().unwrap();
        let stores = LocalStores::open(dir.path()).await.unwrap();
        assert!(dir.path().join(LocalStores::OBJECTS_DIR).is_dir());
        assert!(dir.path().join(LocalStores::META_FILE).is_file());
        let config = stores.create_repo("r", "").await.unwrap();
        assert_eq!(stores.get_repo(&config.id).await.unwrap(), Some(config));
    }

    #[tokio::test]
    async fn a_byte_flipped_on_disk_is_refused_on_read() {
        use crate::object::ObjectStoreExt;
        use rill_types::ObjectKind;

        let dir = tempfile::tempdir().unwrap();
        let stores = LocalStores::open(dir.path()).await.unwrap();
        let hash = stores
            .objects
            .put_blob(bytes::Bytes::from_static(
                b"thirty-two bytes of file content",
            ))
            .await
            .unwrap();
        let hex = hash.to_hex();
        let file = dir
            .path()
            .join(LocalStores::OBJECTS_DIR)
            .join(crate::DEFAULT_NAMESPACE)
            .join(ObjectKind::Blob.as_str())
            .join(&hex[..2])
            .join(&hex);
        let mut bytes = std::fs::read(&file).unwrap();
        *bytes.last_mut().unwrap() ^= 0x01;
        std::fs::write(&file, bytes).unwrap();

        let err = stores
            .objects
            .get(ObjectKind::Blob, &hash)
            .await
            .unwrap_err();
        assert!(matches!(err, crate::Error::Corrupt { .. }), "{err}");
    }
}
