//! Writing an eager mount's base to disk, and applying the file writes an
//! update decides on.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use rill_types::{FileKind, RepoPathBuf};

use super::disk::{prune_empty_dirs, remove_entry, to_fs_path, write_entry};
use crate::error::Result;
use crate::mount::{Change, Mount};
use crate::state::FileStat;
use crate::view::BaseEntry;

/// Files fetched and written at once.
const PARALLEL: usize = 32;
/// Stat entries per state transaction.
const BATCH: usize = 2000;

/// A file change on disk.
#[derive(Debug, Clone)]
pub enum DiskOp {
    /// Write the file.
    Write {
        /// Where.
        path: RepoPathBuf,
        /// Content.
        bytes: Bytes,
        /// Kind.
        kind: FileKind,
    },
    /// Remove the file.
    Remove {
        /// Where.
        path: RepoPathBuf,
    },
}

/// Write every base file of `mount` inside its sparse profile; returns
/// how many were written.
pub(crate) async fn materialize(mount: &Mount) -> Result<u64> {
    let record = mount.record();
    let files = mount
        .base()
        .files(&RepoPathBuf::root(), &record.sparse)
        .await?;
    let total = files.len() as u64;
    let root = record.path.clone();
    let stats = futures::stream::iter(files)
        .map(|file: BaseEntry| {
            let root = root.clone();
            async move {
                let bytes = mount.shared.content.get(&mount.repo, &file.content).await?;
                let fs_path = to_fs_path(&root, &file.path);
                let stat =
                    tokio::task::spawn_blocking(move || write_entry(&fs_path, &bytes, file.kind))
                        .await??;
                Ok::<_, crate::error::Error>((file.path, Some(stat)))
            }
        })
        .buffer_unordered(PARALLEL)
        .try_chunks(BATCH);
    futures::pin_mut!(stats);
    while let Some(batch) = stats.try_next().await.map_err(|e| e.1)? {
        mount
            .write(
                Change {
                    files: batch,
                    ..Change::default()
                },
                None,
            )
            .await?;
    }
    Ok(total)
}

/// Apply `ops` under `root`, returning the stat cache updates.
pub(crate) async fn apply_disk(
    root: &Path,
    ops: Vec<DiskOp>,
) -> Result<Vec<(RepoPathBuf, Option<FileStat>)>> {
    let root: PathBuf = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut out = Vec::with_capacity(ops.len());
        for op in ops {
            match op {
                DiskOp::Write { path, bytes, kind } => {
                    let stat = write_entry(&to_fs_path(&root, &path), &bytes, kind)?;
                    out.push((path, Some(stat)));
                }
                DiskOp::Remove { path } => {
                    let fs_path = to_fs_path(&root, &path);
                    remove_entry(&fs_path)?;
                    prune_empty_dirs(&root, &fs_path);
                    out.push((path, None));
                }
            }
        }
        Ok(out)
    })
    .await?
}
