//! Mapping repository paths onto a staged directory.

use std::path::{Component, Path, PathBuf};

use rill_types::RepoPath;

use crate::error::{Error, Result};

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
