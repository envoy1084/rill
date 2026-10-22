//! On-disk metadata of a checkout: `.rill/checkout.toml` and
//! `.rill/manifest.toml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rill_types::{
    ChangeId, ChangeVersionHash, FileKind, ObjectHash, RepoPathBuf, SparseProfile, WorkspaceId,
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::types::Root;

/// Name of the metadata directory inside a checkout.
pub const META_DIR: &str = ".rill";
const CHECKOUT_FILE: &str = "checkout.toml";
const MANIFEST_FILE: &str = "manifest.toml";

/// Checkout configuration: where it came from and what it is based on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutMeta {
    /// Server endpoint the checkout was created from.
    pub endpoint: String,
    /// Repository name or id.
    pub repo: String,
    /// Server-side workspace, if one is attached.
    pub workspace: Option<WorkspaceId>,
    /// The root the working copy was materialized from.
    pub base: Root,
    /// The change version `base` resolved to, when it had one.
    pub base_version: Option<ChangeVersionHash>,
    /// The change that snapshots amend, once one has been created.
    pub working_change: Option<ChangeId>,
    /// Directories materialized.
    pub sparse: SparseProfile,
}

/// What the checkout knows about one materialized file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// Server-side content identity (`ContentRef::content_hash`).
    pub content_hash: ObjectHash,
    /// BLAKE3 blob digest of the bytes written to disk.
    pub local_hash: ObjectHash,
    /// Size written.
    pub size: u64,
    /// Modification time observed after writing, in epoch milliseconds.
    pub mtime_ms: i64,
    /// File kind.
    pub kind: FileKind,
    /// True when the file holds conflict markers from an update.
    pub conflict: bool,
}

/// Path → entry for every materialized file.
pub type Manifest = BTreeMap<RepoPathBuf, ManifestEntry>;

#[derive(Serialize, Deserialize)]
struct CheckoutFile {
    endpoint: String,
    repo: String,
    workspace: Option<String>,
    base: String,
    base_version: Option<String>,
    working_change: Option<String>,
    sparse: SparseFile,
}

#[derive(Serialize, Deserialize, Default)]
struct SparseFile {
    include: Vec<String>,
    exclude: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct ManifestFile {
    files: BTreeMap<String, EntryFile>,
}

#[derive(Serialize, Deserialize)]
struct EntryFile {
    content_hash: String,
    local_hash: String,
    size: u64,
    mtime_ms: i64,
    kind: String,
    #[serde(default)]
    conflict: bool,
}

impl CheckoutMeta {
    fn to_file(&self) -> CheckoutFile {
        CheckoutFile {
            endpoint: self.endpoint.clone(),
            repo: self.repo.clone(),
            workspace: self.workspace.map(|w| w.to_string()),
            base: self.base.to_string(),
            base_version: self.base_version.map(|h| h.to_hex()),
            working_change: self.working_change.map(|c| c.to_string()),
            sparse: SparseFile {
                include: self.sparse.include.iter().map(|p| p.to_string()).collect(),
                exclude: self.sparse.exclude.iter().map(|p| p.to_string()).collect(),
            },
        }
    }

    fn from_file(f: CheckoutFile) -> Result<Self> {
        let paths = |v: Vec<String>| -> Result<Vec<RepoPathBuf>> {
            v.iter()
                .map(|p| RepoPathBuf::new(p).map_err(Into::into))
                .collect()
        };
        Ok(CheckoutMeta {
            endpoint: f.endpoint,
            repo: f.repo,
            workspace: f.workspace.map(|w| w.parse()).transpose()?,
            base: f.base.parse()?,
            base_version: f
                .base_version
                .map(|h| ObjectHash::from_hex(&h))
                .transpose()?,
            working_change: f.working_change.map(|c| c.parse()).transpose()?,
            sparse: SparseProfile {
                include: paths(f.sparse.include)?,
                exclude: paths(f.sparse.exclude)?,
            },
        })
    }
}

fn kind_name(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Regular => "regular",
        FileKind::Executable => "executable",
        FileKind::Symlink => "symlink",
    }
}

fn kind_from_name(name: &str) -> Result<FileKind> {
    match name {
        "regular" => Ok(FileKind::Regular),
        "executable" => Ok(FileKind::Executable),
        "symlink" => Ok(FileKind::Symlink),
        other => Err(Error::checkout(format!(
            "unknown file kind {other:?} in manifest"
        ))),
    }
}

/// Where a checkout's metadata files live.
pub fn meta_dir(root: &Path) -> PathBuf {
    root.join(META_DIR)
}

/// The file that marks `root` as a checkout.
pub(crate) fn checkout_file(root: &Path) -> PathBuf {
    meta_dir(root).join(CHECKOUT_FILE)
}

/// Write both metadata files. The manifest is written atomically via a
/// temporary file so an interrupted write never leaves a half manifest.
pub fn save(root: &Path, meta: &CheckoutMeta, manifest: &Manifest) -> Result<()> {
    let dir = meta_dir(root);
    std::fs::create_dir_all(&dir)?;
    let checkout = toml::to_string_pretty(&meta.to_file())
        .map_err(|e| Error::checkout(format!("encoding checkout.toml: {e}")))?;
    write_atomic(&dir.join(CHECKOUT_FILE), checkout.as_bytes())?;

    let files = manifest
        .iter()
        .map(|(path, e)| {
            (
                path.to_string(),
                EntryFile {
                    content_hash: e.content_hash.to_hex(),
                    local_hash: e.local_hash.to_hex(),
                    size: e.size,
                    mtime_ms: e.mtime_ms,
                    kind: kind_name(e.kind).to_string(),
                    conflict: e.conflict,
                },
            )
        })
        .collect();
    let manifest_text = toml::to_string(&ManifestFile { files })
        .map_err(|e| Error::checkout(format!("encoding manifest.toml: {e}")))?;
    write_atomic(&dir.join(MANIFEST_FILE), manifest_text.as_bytes())
}

/// Read both metadata files.
pub fn load(root: &Path) -> Result<(CheckoutMeta, Manifest)> {
    let dir = meta_dir(root);
    let checkout_path = dir.join(CHECKOUT_FILE);
    let text = std::fs::read_to_string(&checkout_path)
        .map_err(|e| Error::checkout(format!("{} is not a checkout: {e}", root.display())))?;
    let file: CheckoutFile = toml::from_str(&text)
        .map_err(|e| Error::checkout(format!("{}: {e}", checkout_path.display())))?;
    let meta = CheckoutMeta::from_file(file)?;

    let manifest_path = dir.join(MANIFEST_FILE);
    let manifest_file: ManifestFile = match std::fs::read_to_string(&manifest_path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|e| Error::checkout(format!("{}: {e}", manifest_path.display())))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ManifestFile::default(),
        Err(e) => return Err(e.into()),
    };
    let manifest = manifest_file
        .files
        .into_iter()
        .map(|(path, e)| {
            Ok((
                RepoPathBuf::new(&path)?,
                ManifestEntry {
                    content_hash: ObjectHash::from_hex(&e.content_hash)?,
                    local_hash: ObjectHash::from_hex(&e.local_hash)?,
                    size: e.size,
                    mtime_ms: e.mtime_ms,
                    kind: kind_from_name(&e.kind)?,
                    conflict: e.conflict,
                },
            ))
        })
        .collect::<Result<Manifest>>()?;
    Ok((meta, manifest))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    Ok(crate::files::write_atomic(path, bytes, None)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let meta = CheckoutMeta {
            endpoint: "http://localhost:1".into(),
            repo: "main".into(),
            workspace: Some(WorkspaceId::new()),
            base: Root::trunk(),
            base_version: Some(ObjectHash::ZERO),
            working_change: None,
            sparse: SparseProfile {
                include: vec![RepoPathBuf::new("src").unwrap()],
                exclude: vec![],
            },
        };
        let mut manifest = Manifest::new();
        manifest.insert(
            RepoPathBuf::new("src/a.rs").unwrap(),
            ManifestEntry {
                content_hash: ObjectHash::ZERO,
                local_hash: ObjectHash::ZERO,
                size: 3,
                mtime_ms: 42,
                kind: FileKind::Executable,
                conflict: true,
            },
        );
        save(dir.path(), &meta, &manifest).unwrap();
        let (m2, man2) = load(dir.path()).unwrap();
        assert_eq!(m2, meta);
        assert_eq!(man2, manifest);
    }
}
