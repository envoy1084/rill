//! Walking the working copy, honoring `.rillignore` (gitignore syntax)
//! and skipping the checkout's own files. `.rill/` holds both those and
//! the repository's policy files (`verify.toml`, `owners.toml`, …); only
//! the reserved paths and atomic-write temporaries there are skipped.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use rill_types::RepoPathBuf;

use super::meta::META_DIR;
use crate::error::{Error, Result};
use crate::files::TEMP_PREFIX;

/// Name of the ignore file, honored at every directory level.
pub const IGNORE_FILE: &str = ".rillignore";

/// Every non-ignored file (and symlink) under `root`, as repository paths
/// paired with their absolute locations, sorted by repository path.
///
/// Ignore rules only hide untracked files: callers check tracked paths
/// directly, so a tracked file matching `.rillignore` still reports as
/// modified or deleted.
pub fn walk_files(root: &Path) -> Result<Vec<(RepoPathBuf, PathBuf)>> {
    let mut out = Vec::new();
    let walker = WalkBuilder::new(root)
        .standard_filters(false)
        .hidden(false)
        .add_custom_ignore_filename(IGNORE_FILE)
        .build();
    for entry in walker {
        let entry = entry.map_err(|e| Error::checkout(format!("walking checkout: {e}")))?;
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| Error::checkout("walk produced a path outside the checkout"))?;
        if rel.file_name().is_some_and(|n| n == IGNORE_FILE) {
            continue;
        }
        let repo_path = to_repo_path(rel)?;
        if is_checkout_file(&repo_path) {
            continue;
        }
        out.push((repo_path, entry.into_path()));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// A file the checkout itself keeps under `.rill/`.
fn is_checkout_file(path: &RepoPathBuf) -> bool {
    rill_types::limits::is_reserved(path)
        || (path.parent().as_str() == META_DIR
            && path.file_name().is_some_and(|n| n.starts_with(TEMP_PREFIX)))
}

/// Convert a checkout-relative OS path into a repository path.
pub fn to_repo_path(rel: &Path) -> Result<RepoPathBuf> {
    let mut out = RepoPathBuf::root();
    for component in rel.components() {
        let s = component
            .as_os_str()
            .to_str()
            .ok_or_else(|| Error::checkout(format!("non-UTF-8 path {}", rel.display())))?;
        out.push(s)?;
    }
    Ok(out)
}

/// Convert a repository path into an absolute path under `root`.
pub fn to_fs_path(root: &Path, path: &rill_types::RepoPath) -> PathBuf {
    let mut out = root.to_path_buf();
    for c in path.components() {
        out.push(c);
    }
    out
}
