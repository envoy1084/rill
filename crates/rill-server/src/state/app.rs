//! Process-wide server state shared by every handler.

use std::sync::Arc;

use rill_policy::{Authenticator, Issuer};
use rill_store::Stores;
use tokio_util::sync::CancellationToken;

use super::registry::RepoRegistry;
use crate::auth;
use crate::config::{ServerConfig, StorageConfig};
use crate::error::{Error, Result};

/// Everything a request handler can reach.
pub struct AppState {
    /// Object and metadata stores.
    pub stores: Stores,
    /// The configuration the server started with.
    pub config: Arc<ServerConfig>,
    /// Maps bearer tokens to principals.
    pub auth: Arc<dyn Authenticator>,
    /// Per-repository service graphs.
    pub repos: RepoRegistry,
    /// Signs capability tokens handed out by `IssueToken`.
    pub issuer: Arc<Issuer>,
    /// Signs commit attestations.
    pub attestor: Arc<rill_policy::Signer>,
    /// Cancelled when the server is shutting down; workers watch it.
    pub shutdown: CancellationToken,
    /// Process start time, for health reporting.
    pub started_at: std::time::Instant,
}

impl AppState {
    /// Open the configured storage and build the state. Workers are
    /// spawned lazily per repository (or eagerly via [`RepoRegistry::warm`]).
    pub async fn new(config: ServerConfig, shutdown: CancellationToken) -> Result<Arc<Self>> {
        let stores = open_stores(&config.storage).await?;
        Self::with_stores(config, stores, shutdown)
    }

    /// Build the state over already-open stores.
    pub fn with_stores(
        config: ServerConfig,
        stores: Stores,
        shutdown: CancellationToken,
    ) -> Result<Arc<Self>> {
        let config = Arc::new(config);
        let issuer = Arc::new(auth::build_issuer(&config.auth)?);
        let auth = auth::build_authenticator(&config.auth, &issuer)?;
        let attestor = Arc::new(load_attestor(&config)?);
        let repos = RepoRegistry::new(
            stores.clone(),
            config.clone(),
            attestor.clone(),
            shutdown.clone(),
        );
        Ok(Arc::new(AppState {
            stores,
            config,
            auth,
            repos,
            issuer,
            attestor,
            shutdown,
            started_at: std::time::Instant::now(),
        }))
    }

    /// Whether anonymous requests are allowed and everything is permitted.
    pub fn dev_mode(&self) -> bool {
        self.config.dev_mode
    }
}

/// Open the stores a [`StorageConfig`] describes.
pub async fn open_stores(storage: &StorageConfig) -> Result<Stores> {
    let stores = open_backends(storage).await?;
    stores.check_format().await?;
    Ok(stores)
}

async fn open_backends(storage: &StorageConfig) -> Result<Stores> {
    match storage {
        StorageConfig::Memory => Ok(Stores::memory()),
        StorageConfig::Local { dir } => Ok(rill_store::LocalStores::open(dir).await?),
        StorageConfig::S3 {
            url,
            prefix,
            meta_dir,
            allow_unconditional_put,
        } => {
            let objects = rill_store::ObjectStoreBackend::from_url(url, prefix)?
                .probe(*allow_unconditional_put)
                .await?;
            let meta_path = meta_dir.join(rill_store::LocalStores::META_FILE);
            tokio::fs::create_dir_all(meta_dir)
                .await
                .map_err(|e| Error::Config(format!("creating {}: {e}", meta_dir.display())))?;
            let meta = rill_store::RedbMetaStore::open(meta_path).await?;
            Ok(Stores::new(Arc::new(objects), Arc::new(meta)))
        }
    }
}

/// The key commit attestations are signed with.
fn load_attestor(config: &ServerConfig) -> Result<rill_policy::Signer> {
    match &config.attestation_key {
        Some(path) => {
            let contents = std::fs::read_to_string(path)
                .map_err(|e| Error::Config(format!("reading {}: {e}", path.display())))?;
            rill_policy::KeyFile::decode(contents.trim())
                .map_err(|e| Error::Config(format!("{}: {e}", path.display())))
        }
        None => {
            if !config.dev_mode {
                tracing::warn!(
                    "no attestation_key configured; commit attestations use a per-process key"
                );
            }
            Ok(rill_policy::Signer::generate())
        }
    }
}
