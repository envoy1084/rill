//! Writing an input tree to a directory on disk.
//!
//! Content is fetched on the async side and written on blocking threads in
//! batches, so a large tree neither blocks the runtime nor spawns a task
//! per file.

use std::path::{Component, Path, PathBuf};

use bytes::Bytes;
use rill_core::{read_content, TreeOps};
use rill_types::{FileKind, RepoPath, RepoPathBuf, SparseProfile, TreeHash};

use crate::error::{Error, Result};

/// Number of files handed to one blocking write task.
const WRITE_BATCH: usize = 256;

/// Options for [`materialize`].
#[derive(Debug, Clone, Default)]
pub struct MaterializeOptions {
    /// Which parts of the tree to write; the default is everything.
    pub sparse: SparseProfile,
}

/// A file written to disk by [`materialize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedFile {
    pub path: RepoPathBuf,
    pub kind: FileKind,
    pub size: u64,
}

/// Write the files of `root` beneath `dest`, creating directories as
/// needed, setting executable bits and creating symlinks.
///
/// Returns the files written in path order.
pub async fn materialize(
    trees: &TreeOps,
    root: TreeHash,
    dest: &Path,
    options: &MaterializeOptions,
) -> Result<Vec<MaterializedFile>> {
    let dest = dest.to_path_buf();
    let dest_for_mkdir = dest.clone();
    tokio::task::spawn_blocking(move || std::fs::create_dir_all(&dest_for_mkdir)).await??;

    let entries = trees
        .walk(root, &RepoPathBuf::root(), &options.sparse)
        .await?;
    let mut written = Vec::with_capacity(entries.len());
    let objects = trees.objects();

    for chunk in entries.chunks(WRITE_BATCH) {
        let mut batch = Vec::with_capacity(chunk.len());
        for entry in chunk {
            let bytes = read_content(objects.as_ref(), &entry.content).await?;
            written.push(MaterializedFile {
                path: entry.path.clone(),
                kind: entry.kind,
                size: bytes.len() as u64,
            });
            batch.push((local_path(&dest, &entry.path)?, entry.kind, bytes));
        }
        tokio::task::spawn_blocking(move || write_batch(batch)).await??;
    }
    Ok(written)
}

/// Resolve a repository path under `root`, refusing anything that would
/// leave it. Repository paths are validated to contain no `..`, so this
/// is a belt-and-braces check against a corrupt tree.
pub fn local_path(root: &Path, path: &RepoPath) -> Result<PathBuf> {
    let mut out = root.to_path_buf();
    for component in path.components() {
        let piece = Path::new(component);
        if piece
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(Error::PathEscape(root.join(path.as_str())));
        }
        out.push(piece);
    }
    Ok(out)
}

fn write_batch(batch: Vec<(PathBuf, FileKind, Bytes)>) -> std::io::Result<()> {
    for (path, kind, bytes) in batch {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // A previous symlink or file at the path must not be followed or
        // appended to; start from nothing.
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&path)?,
            Ok(_) => std::fs::remove_file(&path)?,
            Err(_) => {}
        }
        match kind {
            FileKind::Regular | FileKind::Executable => {
                std::fs::write(&path, &bytes)?;
                set_executable(&path, kind.is_executable())?;
            }
            FileKind::Symlink => {
                let target = String::from_utf8_lossy(&bytes).into_owned();
                make_symlink(&target, &path)?;
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if executable { 0o755 } else { 0o644 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn make_symlink(target: &str, path: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, path)
}

#[cfg(not(unix))]
fn make_symlink(target: &str, path: &Path) -> std::io::Result<()> {
    // Without symlink support the link is written as a file holding its
    // target, which keeps builds reproducible even if not faithful.
    std::fs::write(path, target)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rill_core::tree::{Edit, TreeBuilder};
    use rill_store::MemoryObjectStore;
    use rill_types::ContentRef;

    use super::*;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    #[tokio::test]
    async fn round_trips_a_small_tree() {
        let ops = TreeOps::new(Arc::new(MemoryObjectStore::new()), 16);
        let mut b = TreeBuilder::new();
        b.edit(
            &p("src/main.rs"),
            Edit::Set(
                ContentRef::inline(b"fn main(){}".to_vec()),
                FileKind::Regular,
            ),
        );
        b.edit(
            &p("run.sh"),
            Edit::Set(
                ContentRef::inline(b"#!/bin/sh\n".to_vec()),
                FileKind::Executable,
            ),
        );
        b.edit(
            &p("link"),
            Edit::Set(ContentRef::inline(b"run.sh".to_vec()), FileKind::Symlink),
        );
        let root = b.build(&ops, None).await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        let written = materialize(&ops, root, dir.path(), &MaterializeOptions::default())
            .await
            .unwrap();
        assert_eq!(written.len(), 3);
        assert_eq!(
            std::fs::read(dir.path().join("src/main.rs")).unwrap(),
            b"fn main(){}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("run.sh"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
            assert_eq!(
                std::fs::read_link(dir.path().join("link")).unwrap(),
                PathBuf::from("run.sh")
            );
        }

        let sparse = MaterializeOptions {
            sparse: SparseProfile {
                include: vec![p("src")],
                exclude: vec![],
            },
        };
        let dir2 = tempfile::tempdir().unwrap();
        let written = materialize(&ops, root, dir2.path(), &sparse).await.unwrap();
        assert_eq!(written.len(), 1);
        assert!(!dir2.path().join("run.sh").exists());
    }
}
