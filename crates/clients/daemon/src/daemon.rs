//! The daemon: its mounts, created, restored at start and removed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use rill_sdk::Root;
use rill_types::{RepoPathBuf, SparseProfile};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::cache::CacheStats;
use crate::config::DaemonConfig;
use crate::eager::{materialize, Watch, MARKER};
use crate::error::{Error, Result};
use crate::mount::{resolve_base, Change, Mount, MountMode, MountRecord};
use crate::shared::Shared;

/// What to check out.
#[derive(Debug, Clone)]
pub struct MountSpec {
    /// Directory (created; must be empty or absent).
    pub path: PathBuf,
    /// Server endpoint.
    pub endpoint: String,
    /// A token for the endpoint, kept in memory.
    pub token: Option<String>,
    /// Repository.
    pub repo: String,
    /// Revision; trunk when `None`.
    pub revision: Option<Root>,
    /// Directories to include; everything when empty.
    pub sparse: Vec<RepoPathBuf>,
    /// How to present files.
    pub mode: MountMode,
    /// Skip the server workspace.
    pub no_workspace: bool,
}

/// The marker at a mount's root, so tools find the mount and its daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    /// `mounts/{id}`.
    pub mount: String,
    /// The daemon directory serving it.
    pub daemon: PathBuf,
}

impl Marker {
    /// Walk up from `start` to the nearest mount marker.
    pub fn find(start: &Path) -> Option<(PathBuf, Marker)> {
        start.ancestors().find_map(|dir| {
            let text = std::fs::read_to_string(dir.join(MARKER)).ok()?;
            Some((dir.to_path_buf(), serde_json::from_str(&text).ok()?))
        })
    }
}

/// The running daemon.
pub struct Daemon {
    shared: Arc<Shared>,
    mounts: RwLock<BTreeMap<String, Arc<Mount>>>,
    shutdown: CancellationToken,
}

impl Daemon {
    /// Open the daemon's state and resume every stored mount.
    pub async fn open(config: DaemonConfig) -> Result<Arc<Daemon>> {
        let shared = Arc::new(tokio::task::spawn_blocking(move || Shared::open(config)).await??);
        let daemon = Arc::new(Daemon {
            shared: shared.clone(),
            mounts: RwLock::new(BTreeMap::new()),
            shutdown: CancellationToken::new(),
        });
        for record in shared.db(|db| db.mounts()).await? {
            let id = record.id.clone();
            match daemon.resume(record).await {
                Ok(mount) => {
                    daemon.mounts.write().insert(id, mount);
                }
                Err(e) => tracing::error!(mount = %id, error = %e, "mount not resumed"),
            }
        }
        Ok(daemon)
    }

    /// Cancelled when the daemon should stop.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// The configuration.
    pub fn config(&self) -> &DaemonConfig {
        &self.shared.config
    }

    /// Object cache counters.
    pub fn cache_stats(&self) -> CacheStats {
        self.shared.content.cache().stats()
    }

    /// Every mount.
    pub fn mounts(&self) -> Vec<Arc<Mount>> {
        self.mounts.read().values().cloned().collect()
    }

    /// A mount by name (`mounts/{id}`) or id.
    pub fn mount(&self, name: &str) -> Result<Arc<Mount>> {
        let id = name.strip_prefix("mounts/").unwrap_or(name);
        self.mounts
            .read()
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("no mount {name}")))
    }

    /// The mount whose directory contains `path`.
    pub fn find(&self, path: &Path) -> Result<Arc<Mount>> {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.mounts
            .read()
            .values()
            .filter(|m| path.starts_with(&m.record().path))
            .max_by_key(|m| m.record().path.as_os_str().len())
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("{} is not inside a mount", path.display())))
    }

    /// Offer a caller's token for `endpoint`.
    pub fn offer_token(&self, endpoint: &str, token: &str) {
        self.shared.clients.offer_token(endpoint, token);
    }

    /// Check out `spec`; returns the mount and how many files it shows.
    pub async fn create_mount(&self, spec: MountSpec) -> Result<(Arc<Mount>, u64)> {
        if !spec.path.is_absolute() {
            return Err(Error::Invalid(format!(
                "{} is not an absolute path",
                spec.path.display()
            )));
        }
        if spec.mode != MountMode::Eager {
            return Err(Error::Unsupported(format!(
                "{:?} mounts need the Linux virtual filesystem",
                spec.mode
            )));
        }
        let path = prepare_dir(&spec.path).await?;
        if self.find(&path).is_ok() {
            return Err(Error::Conflict(format!(
                "{} is inside another mount",
                path.display()
            )));
        }
        if let Some(token) = &spec.token {
            self.offer_token(&spec.endpoint, token);
        }
        let client = self.shared.clients.get(&spec.endpoint)?;
        let repo = client.repo(spec.repo.clone());
        repo.info().await?;
        let revision = spec.revision.clone().unwrap_or_else(Root::trunk);
        let (base_version, base_tree) = resolve_base(&repo, &revision).await?;
        let sparse = SparseProfile {
            include: spec.sparse.clone(),
            exclude: Vec::new(),
        };
        let workspace = if spec.no_workspace || spec.mode == MountMode::Snapshot {
            None
        } else {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "checkout".into());
            let created = repo
                .workspaces()
                .create(&name, base_version.map(Root::Version), sparse.clone())
                .await?;
            Some(created.workspace.id)
        };
        let record = MountRecord {
            id: uuid::Uuid::new_v4().simple().to_string(),
            path: path.clone(),
            endpoint: spec.endpoint,
            repo: spec.repo,
            mode: spec.mode,
            workspace,
            base: revision.to_string(),
            base_version,
            base_tree,
            working_change: None,
            sparse,
            etag: None,
            synced: 0,
        };
        let mount = Arc::new(Mount::new(
            self.shared.clone(),
            repo,
            record.clone(),
            vec![],
        ));
        mount
            .write(
                Change {
                    record: Some(record.clone()),
                    ..Change::default()
                },
                None,
            )
            .await?;
        let files = materialize(&mount).await?;
        write_marker(&path, &record, &self.shared.config.dir).await?;
        self.start(&mount)?;
        self.mounts.write().insert(record.id.clone(), mount.clone());
        Ok((mount, files))
    }

    /// Stop serving a mount and forget it; an eager mount's files stay.
    pub async fn delete_mount(&self, name: &str) -> Result<()> {
        let mount = self.mount(name)?;
        mount.cancel.cancel();
        let record = mount.record();
        self.mounts.write().remove(&record.id);
        let id = record.id.clone();
        self.shared.db(move |db| db.delete_mount(&id)).await?;
        for (_, entry) in mount.entries() {
            if let Some(d) = entry.digest() {
                self.shared.content.cache().unpin(&d);
            }
        }
        let marker = record.path.join(MARKER);
        let _ = tokio::task::spawn_blocking(move || std::fs::remove_file(marker)).await?;
        Ok(())
    }

    async fn resume(&self, record: MountRecord) -> Result<Arc<Mount>> {
        let client = self.shared.clients.get(&record.endpoint)?;
        let repo = client.repo(record.repo.clone());
        let id = record.id.clone();
        let entries = self.shared.db(move |db| db.overlay(&id)).await?;
        let mount = Arc::new(Mount::new(self.shared.clone(), repo, record, entries));
        self.start(&mount)?;
        if mount.pending_count() > 0 {
            mount.dirty.notify_one();
        }
        Ok(mount)
    }

    fn start(&self, mount: &Arc<Mount>) -> Result<()> {
        if mount.mode() == MountMode::Eager {
            let watch = Watch::start(mount)?;
            let _ = mount.watch.set(watch);
        }
        crate::sync::task::spawn(mount.clone());
        Ok(())
    }

    /// Stop every mount's background work.
    pub fn stop(&self) {
        for mount in self.mounts() {
            mount.cancel.cancel();
        }
        self.shutdown.cancel();
    }
}

/// Create `path` if absent, refuse a non-empty one, and canonicalize it.
async fn prepare_dir(path: &Path) -> Result<PathBuf> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&path)?;
        if std::fs::read_dir(&path)?.next().is_some() {
            return Err(Error::Conflict(format!("{} is not empty", path.display())));
        }
        Ok(std::fs::canonicalize(&path)?)
    })
    .await?
}

async fn write_marker(
    root: &Path,
    record: &MountRecord,
    dir: &crate::config::DaemonDir,
) -> Result<()> {
    let marker = Marker {
        mount: record.name(),
        daemon: dir.root().to_path_buf(),
    };
    let text = serde_json::to_vec_pretty(&marker).map_err(|e| Error::Invalid(e.to_string()))?;
    let path = root.join(MARKER);
    tokio::task::spawn_blocking(move || crate::cache::write_atomically(&path, &text)).await?
}
