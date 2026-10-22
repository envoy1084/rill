//! Writing an input root from an execution space to disk.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use prost::Message;
use rill_reapi_proto::reapi::Directory;

use crate::error::{Error, Result};
use crate::remote::{BlobDigest, ExecSpace};

/// Files handed to one blocking write task; the tasks run concurrently.
const WRITE_BATCH: usize = 64;

/// Distinct blobs fetched at once.
const FETCH_CONCURRENCY: usize = 16;

enum Entry {
    File {
        path: PathBuf,
        bytes: Bytes,
        executable: bool,
    },
    Symlink {
        path: PathBuf,
        target: String,
    },
    Dir(PathBuf),
}

/// Write the input root `root` beneath `dest`: the directories are read
/// first, then each distinct file content is fetched once, concurrently,
/// and the files are written.
pub(crate) async fn stage(space: &ExecSpace, root: BlobDigest, dest: &Path) -> Result<()> {
    let mut pending = vec![(dest.to_path_buf(), root)];
    let mut dirs = Vec::new();
    let mut files: Vec<(PathBuf, BlobDigest, bool)> = Vec::new();
    let mut links = Vec::new();
    while let Some((dir_path, digest)) = pending.pop() {
        let bytes = space
            .read(&digest)
            .await?
            .ok_or_else(|| Error::MissingBlob(digest.to_string()))?;
        let dir = Directory::decode(bytes)
            .map_err(|e| Error::Codec(format!("directory {digest}: {e}")))?;
        for file in &dir.files {
            let digest = BlobDigest::parse(
                file.digest
                    .as_ref()
                    .ok_or_else(|| Error::InvalidDigest(file.name.clone()))?,
            )?;
            files.push((child(&dir_path, &file.name)?, digest, file.is_executable));
        }
        for link in &dir.symlinks {
            links.push(Entry::Symlink {
                path: child(&dir_path, &link.name)?,
                target: link.target.clone(),
            });
        }
        for sub in &dir.directories {
            let digest = BlobDigest::parse(
                sub.digest
                    .as_ref()
                    .ok_or_else(|| Error::InvalidDigest(sub.name.clone()))?,
            )?;
            pending.push((child(&dir_path, &sub.name)?, digest));
        }
        dirs.push(Entry::Dir(dir_path));
    }
    let distinct: HashSet<BlobDigest> = files.iter().map(|(_, d, _)| *d).collect();
    let contents: HashMap<BlobDigest, Bytes> = futures::stream::iter(distinct)
        .map(|digest| async move {
            let bytes = space
                .read(&digest)
                .await?
                .ok_or_else(|| Error::MissingBlob(digest.to_string()))?;
            Ok::<_, Error>((digest, bytes))
        })
        .buffer_unordered(FETCH_CONCURRENCY)
        .try_collect()
        .await?;
    // Directories first, then files and links from several threads.
    tokio::task::spawn_blocking(move || write(dirs)).await??;
    let mut entries: Vec<Entry> = files
        .into_iter()
        .map(|(path, digest, executable)| Entry::File {
            path,
            // Fetched above for every digest named.
            bytes: contents[&digest].clone(),
            executable,
        })
        .collect();
    entries.extend(links);
    let writes = chunks(entries, WRITE_BATCH)
        .into_iter()
        .map(|batch| tokio::task::spawn_blocking(move || write(batch)));
    for written in futures::future::join_all(writes).await {
        written??;
    }
    Ok(())
}

fn chunks(mut entries: Vec<Entry>, size: usize) -> Vec<Vec<Entry>> {
    let mut out = Vec::new();
    while entries.len() > size {
        let rest = entries.split_off(size);
        out.push(entries);
        entries = rest;
    }
    out.push(entries);
    out
}

/// `name` inside `dir`; a name that is not one plain component is refused.
fn child(dir: &Path, name: &str) -> Result<PathBuf> {
    let plain = !name.is_empty() && name != "." && name != ".." && !name.contains('/');
    if !plain {
        return Err(Error::PathEscape(dir.join(name)));
    }
    Ok(dir.join(name))
}

fn write(entries: Vec<Entry>) -> std::io::Result<()> {
    for entry in entries {
        match entry {
            Entry::Dir(path) => std::fs::create_dir_all(path)?,
            Entry::File {
                path,
                bytes,
                executable,
            } => {
                std::fs::write(&path, &bytes)?;
                set_executable(&path, executable)?;
            }
            Entry::Symlink { path, target } => make_symlink(&target, &path)?,
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
    std::fs::write(path, target)
}
