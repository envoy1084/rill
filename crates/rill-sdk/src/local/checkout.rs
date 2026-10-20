//! The `Checkout` handle: create, open, status, edits and snapshots.

use std::path::{Path, PathBuf};

use rill_types::{ChangeVersionHash, RepoPath, RepoPathBuf, SparseProfile, WorkspaceId};

use super::fs::{local_hash, read_entry, stat, write_entry};
use super::meta::{self, CheckoutMeta, Manifest, ManifestEntry, META_DIR};
use super::remote::{self, RemoteFile};
use super::status::{compute_status, LocalChange, LocalChangeKind};
use super::walk::to_fs_path;
use crate::client::Client;
use crate::error::{Error, Result};
use crate::repo::RepoClient;
use crate::types::{ApplyOptions, Edit, EditBase, Root, VersionResult};

/// A local working copy of part of a repository, backed by `.rill/`.
#[derive(Debug)]
pub struct Checkout {
    root: PathBuf,
    repo: RepoClient,
    meta: CheckoutMeta,
    manifest: Manifest,
}

impl Checkout {
    /// Materialize `root` (restricted to `sparse`) into `dir`, which must
    /// be empty or absent, and write the checkout metadata.
    pub async fn create(
        dir: impl AsRef<Path>,
        repo: RepoClient,
        root: Root,
        sparse: SparseProfile,
    ) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        if meta::meta_dir(&dir).exists() {
            return Err(Error::checkout(format!(
                "{} is already a checkout",
                dir.display()
            )));
        }
        tokio::fs::create_dir_all(&dir).await?;

        let files = remote::list_tree(&repo, &root, &sparse).await?;
        let mut manifest = Manifest::new();
        for (path, file) in &files {
            let entry = materialize(&repo, &dir, path, file).await?;
            manifest.insert(path.clone(), entry);
        }
        let base_version = resolve_version(&repo, &root).await?;
        let meta = CheckoutMeta {
            endpoint: repo.client().config().endpoint.clone(),
            repo: repo.name().to_string(),
            workspace: None,
            base: root,
            base_version,
            working_change: None,
            sparse,
        };
        let checkout = Checkout {
            root: dir,
            repo,
            meta,
            manifest,
        };
        checkout.save().await?;
        Ok(checkout)
    }

    /// Open an existing checkout with a connected client. The repository
    /// is taken from the checkout's metadata.
    pub async fn open(dir: impl AsRef<Path>, client: Client) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        let (meta, manifest) = {
            let dir = dir.clone();
            tokio::task::spawn_blocking(move || meta::load(&dir))
                .await
                .map_err(|e| Error::checkout(format!("load task failed: {e}")))??
        };
        let repo = client.repo(meta.repo.clone());
        Ok(Checkout {
            root: dir,
            repo,
            meta,
            manifest,
        })
    }

    /// Read a checkout's metadata without connecting, so a caller can
    /// build the right client first.
    pub fn read_meta(dir: impl AsRef<Path>) -> Result<CheckoutMeta> {
        Ok(meta::load(dir.as_ref())?.0)
    }

    /// Walk up from `start` to the nearest directory containing `.rill/`.
    pub fn find(start: impl AsRef<Path>) -> Option<PathBuf> {
        let mut dir = Some(start.as_ref().to_path_buf());
        while let Some(d) = dir {
            if d.join(META_DIR).is_dir() {
                return Some(d);
            }
            dir = d.parent().map(Path::to_path_buf);
        }
        None
    }

    /// The checkout directory.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// The repository client the checkout talks to.
    pub fn repo(&self) -> &RepoClient {
        &self.repo
    }

    /// Checkout metadata.
    pub fn meta(&self) -> &CheckoutMeta {
        &self.meta
    }

    /// The root the working copy is based on.
    pub fn base(&self) -> &Root {
        &self.meta.base
    }

    /// The tracked files.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Absolute path of a repository path inside the checkout.
    pub fn fs_path(&self, path: &RepoPath) -> PathBuf {
        to_fs_path(&self.root, path)
    }

    /// Attach a server-side workspace id.
    pub async fn set_workspace(&mut self, workspace: Option<WorkspaceId>) -> Result<()> {
        self.meta.workspace = workspace;
        self.save().await
    }

    /// Paths that differ from the base.
    pub async fn status(&self) -> Result<Vec<LocalChange>> {
        let root = self.root.clone();
        let manifest = self.manifest.clone();
        tokio::task::spawn_blocking(move || compute_status(&root, &manifest))
            .await
            .map_err(|e| Error::checkout(format!("status task failed: {e}")))?
    }

    /// The edits that turn the base into the working copy: one `Set` per
    /// added or modified file, one `Delete` per deleted file. Conflicted
    /// files are an error until marked resolved.
    pub async fn edits(&self) -> Result<Vec<Edit>> {
        let changes = self.status().await?;
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || edits_for(&root, &changes))
            .await
            .map_err(|e| Error::checkout(format!("edits task failed: {e}")))?
    }

    /// Upload the working copy's changes as a change version on top of
    /// the base. The first snapshot creates a change; later ones amend it
    /// (unless `opts.change_id` names another change).
    pub async fn snapshot(&mut self, mut opts: ApplyOptions) -> Result<VersionResult> {
        let edits = self.edits().await?;
        if opts.change_id.is_none() {
            opts.change_id = self.meta.working_change;
        }
        let result = self
            .repo
            .apply_edits(EditBase::Root(self.meta.base.clone()), edits, opts)
            .await?;
        self.meta.working_change = Some(result.version.change_id);
        self.save().await?;
        Ok(result)
    }

    /// Clear the conflict flag on paths the user has resolved by hand.
    pub async fn mark_resolved(&mut self, paths: &[RepoPathBuf]) -> Result<()> {
        for path in paths {
            if let Some(entry) = self.manifest.get_mut(path) {
                entry.conflict = false;
            }
        }
        self.save().await
    }

    pub(crate) fn manifest_mut(&mut self) -> &mut Manifest {
        &mut self.manifest
    }

    pub(crate) fn meta_mut(&mut self) -> &mut CheckoutMeta {
        &mut self.meta
    }

    pub(crate) async fn save(&self) -> Result<()> {
        let root = self.root.clone();
        let meta = self.meta.clone();
        let manifest = self.manifest.clone();
        tokio::task::spawn_blocking(move || meta::save(&root, &meta, &manifest))
            .await
            .map_err(|e| Error::checkout(format!("save task failed: {e}")))?
    }
}

/// Fetch a remote file, write it and build its manifest entry.
pub(crate) async fn materialize(
    repo: &RepoClient,
    root: &Path,
    path: &RepoPath,
    file: &RemoteFile,
) -> Result<ManifestEntry> {
    let bytes = remote::fetch(repo, file).await?;
    let fs_path = to_fs_path(root, path);
    let kind = file.kind;
    let content_hash = file.content.content_hash();
    tokio::task::spawn_blocking(move || {
        write_entry(&fs_path, &bytes, kind)?;
        let st = stat(&fs_path)?.ok_or_else(|| {
            Error::checkout(format!("{} vanished after write", fs_path.display()))
        })?;
        Ok(ManifestEntry {
            content_hash,
            local_hash: local_hash(&bytes),
            size: st.size,
            mtime_ms: st.mtime_ms,
            kind,
            conflict: false,
        })
    })
    .await
    .map_err(|e| Error::checkout(format!("write task failed: {e}")))?
}

/// Resolve a root to its change version, if it has one.
pub(crate) async fn resolve_version(
    repo: &RepoClient,
    root: &Root,
) -> Result<Option<ChangeVersionHash>> {
    match root {
        Root::Tree(_) => Ok(None),
        other => repo.resolve_root(other).await.map(Some),
    }
}

fn edits_for(root: &Path, changes: &[LocalChange]) -> Result<Vec<Edit>> {
    let mut edits = Vec::with_capacity(changes.len());
    for change in changes {
        match change.kind {
            LocalChangeKind::Added | LocalChangeKind::Modified => {
                let (bytes, kind) = read_entry(&to_fs_path(root, &change.path))?;
                edits.push(Edit::Set {
                    path: change.path.clone(),
                    bytes,
                    kind,
                });
            }
            LocalChangeKind::Deleted => edits.push(Edit::Delete {
                path: change.path.clone(),
            }),
            LocalChangeKind::Conflict => {
                return Err(Error::checkout(format!(
                    "{} has unresolved conflicts",
                    change.path
                )))
            }
        }
    }
    Ok(edits)
}
