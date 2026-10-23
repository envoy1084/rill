//! Bringing the overlay in line with what is on disk at some paths. A
//! file whose stat matches the stat cache is not read; one whose content
//! matches what the working copy already has records nothing.

use std::collections::BTreeSet;
use std::path::Path;

use ignore::WalkBuilder;
use rill_types::{RepoPath, RepoPathBuf};

use super::disk::{read_entry, stat, to_fs_path, to_repo_path, Found};
use super::ignore::{IgnoreRules, IGNORE_FILE};
use crate::cache::local_digest;
use crate::error::Result;
use crate::mount::{Change, Mount};
use crate::overlay::OverlayOp;

/// Reconcile every path in `paths` (files or directories).
pub(crate) async fn reconcile(
    mount: &Mount,
    rules: &IgnoreRules,
    paths: BTreeSet<RepoPathBuf>,
) -> Result<()> {
    let root = mount.record().path;
    let mut files = BTreeSet::new();
    for path in paths {
        if IgnoreRules::is_internal(&path) {
            continue;
        }
        let fs_path = to_fs_path(&root, &path);
        let found = tokio::task::spawn_blocking(move || stat(&fs_path)).await??;
        if found == Found::Dir {
            let dir = to_fs_path(&root, &path);
            let root2 = root.clone();
            let on_disk = tokio::task::spawn_blocking(move || walk_files(&root2, &dir)).await??;
            files.extend(on_disk);
        }
        files.extend(known_under(mount, &path).await?);
        files.insert(path);
    }
    let mut change = Change::default();
    for path in files {
        if IgnoreRules::is_internal(&path) {
            continue;
        }
        reconcile_file(mount, rules, &root, &path, &mut change).await?;
    }
    mount.write(change, None).await
}

/// Every non-ignored file under `dir`, as repository paths.
pub(crate) fn walk_files(root: &Path, dir: &Path) -> Result<Vec<RepoPathBuf>> {
    let mut out = Vec::new();
    let walker = WalkBuilder::new(dir)
        .standard_filters(false)
        .hidden(false)
        .add_custom_ignore_filename(IGNORE_FILE)
        .build();
    for entry in walker {
        let entry = entry.map_err(|e| crate::error::Error::Invalid(format!("walk: {e}")))?;
        if entry.file_type().is_none_or(|t| t.is_dir()) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        let path = to_repo_path(rel)?;
        if !IgnoreRules::is_internal(&path) && path.file_name() != Some(IGNORE_FILE) {
            out.push(path);
        }
    }
    Ok(out)
}

/// Files the working copy has at or below `path`: in the base (inside
/// the sparse profile) or the overlay.
async fn known_under(mount: &Mount, path: &RepoPath) -> Result<Vec<RepoPathBuf>> {
    let sparse = mount.record().sparse;
    let mut out: Vec<RepoPathBuf> = mount
        .base()
        .files(path, &sparse)
        .await?
        .into_iter()
        .map(|f| f.path)
        .collect();
    out.extend(mount.entries_under(path).into_iter().map(|(p, _)| p));
    Ok(out)
}

async fn reconcile_file(
    mount: &Mount,
    rules: &IgnoreRules,
    root: &Path,
    path: &RepoPathBuf,
    change: &mut Change,
) -> Result<()> {
    let fs_path = to_fs_path(root, path);
    let found = {
        let fs_path = fs_path.clone();
        tokio::task::spawn_blocking(move || stat(&fs_path)).await??
    };
    let current = mount.current(path).await?;
    let entry = mount.entry(path);
    let conflict = entry.is_some_and(|e| e.conflict);
    let Found::File {
        size,
        mtime_ns,
        kind,
    } = found
    else {
        if current.is_some() {
            change
                .overlay
                .push((path.clone(), Some((OverlayOp::Delete, conflict))));
        }
        change.files.push((path.clone(), None));
        return Ok(());
    };
    let tracked = current.is_some() || entry.is_some();
    if !tracked && rules.ignored(path, false) {
        return Ok(());
    }
    let cached = {
        let (id, path) = (mount.id(), path.clone());
        mount.shared.db(move |db| db.file(&id, &path)).await?
    };
    let unchanged_stat =
        cached.is_some_and(|c| c.size == size && c.mtime_ns == mtime_ns && c.kind == kind);
    if let (true, Some(c), Some(cur)) = (unchanged_stat, cached, current) {
        if c.digest == cur.digest && c.kind == cur.kind {
            return Ok(());
        }
    }
    let (bytes, kind) = tokio::task::spawn_blocking(move || read_entry(&fs_path)).await??;
    let digest = local_digest(&bytes);
    let stat = crate::state::FileStat {
        size,
        mtime_ns,
        digest,
        kind,
    };
    change.files.push((path.clone(), Some(stat)));
    if current.is_some_and(|c| c.digest == digest && c.kind == kind) {
        return Ok(());
    }
    let size = bytes.len() as u64;
    mount.shared.content.put_local(bytes).await?;
    change.overlay.push((
        path.clone(),
        Some((OverlayOp::Set { digest, kind, size }, conflict)),
    ));
    Ok(())
}
