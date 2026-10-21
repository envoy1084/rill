//! Opening the stores a configuration describes.

use std::path::Path;
use std::sync::Arc;

use rill_store::{LocalObjectStore, LocalStores, MetaStore, RedbMetaStore, Stores};

use crate::config::{MetadataConfig, StorageConfig};
use crate::error::{Error, Result};

/// Open the stores the configuration describes and check their format.
pub async fn open_stores(storage: &StorageConfig, metadata: &MetadataConfig) -> Result<Stores> {
    let meta = open_metadata(metadata)?;
    let stores = match storage {
        StorageConfig::Memory => Stores::new(
            Arc::new(rill_store::MemoryObjectStore::new()),
            meta.unwrap_or_else(|| Arc::new(rill_store::MemoryMetaStore::new())),
        ),
        StorageConfig::Local { dir } => match meta {
            None => LocalStores::open(dir).await?,
            Some(meta) => Stores::new(
                Arc::new(LocalObjectStore::open(dir.join(LocalStores::OBJECTS_DIR)).await?),
                meta,
            ),
        },
        StorageConfig::S3 {
            url,
            prefix,
            meta_dir,
            allow_unconditional_put,
        } => {
            let objects = rill_store::ObjectStoreBackend::from_url(url, prefix)?
                .probe(*allow_unconditional_put)
                .await?;
            let meta = match (meta, meta_dir) {
                (Some(meta), _) => meta,
                (None, Some(dir)) => open_redb(dir).await?,
                (None, None) => {
                    return Err(Error::Config(
                        "s3 storage needs meta_dir unless metadata is in FoundationDB".into(),
                    ))
                }
            };
            Stores::new(Arc::new(objects), meta)
        }
    };
    stores.check_format().await?;
    stores.ensure_default_org().await?;
    Ok(stores)
}

async fn open_redb(dir: &Path) -> Result<Arc<dyn MetaStore>> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| Error::Config(format!("creating {}: {e}", dir.display())))?;
    Ok(Arc::new(
        RedbMetaStore::open(dir.join(LocalStores::META_FILE)).await?,
    ))
}

/// The metadata store `[metadata]` names, or `None` to keep it beside the
/// objects.
fn open_metadata(metadata: &MetadataConfig) -> Result<Option<Arc<dyn MetaStore>>> {
    match metadata {
        MetadataConfig::Storage => Ok(None),
        #[cfg(feature = "fdb")]
        MetadataConfig::Fdb { cluster_file, root } => {
            let cluster = cluster_file
                .as_ref()
                .map(|p| {
                    p.to_str()
                        .map(str::to_string)
                        .ok_or_else(|| Error::Config(format!("{} is not UTF-8", p.display())))
                })
                .transpose()?;
            let store = rill_store::FdbMetaStore::connect(cluster.as_deref(), root)?;
            tracing::info!(root = %root, "metadata in FoundationDB");
            Ok(Some(Arc::new(store)))
        }
        #[cfg(not(feature = "fdb"))]
        MetadataConfig::Fdb { .. } => Err(Error::Config(
            "this server was built without FoundationDB support (feature fdb)".into(),
        )),
    }
}
