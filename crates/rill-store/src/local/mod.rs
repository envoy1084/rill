//! Local on-disk backends: objects as files, metadata in a redb database.
//!
//! This is the single-node deployment: everything under one directory,
//! no services to run. [`LocalStores::open`] wires both halves together.

mod meta;
mod objects;

use std::path::Path;
use std::sync::Arc;

pub use meta::RedbMetaStore;
pub use objects::LocalObjectStore;

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
}
