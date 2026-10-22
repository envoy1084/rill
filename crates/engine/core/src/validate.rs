//! Checks a change version must pass before it is stored.

use rill_store::ObjectStore;
use rill_types::limits::{check_change, check_symlink_target, is_windows_reserved};
use rill_types::{ChangeVersion, ContentRef, FileChange, FileKind};

use crate::blobs::read_content;
use crate::error::{Error, Result};

/// Repository-level path rules beyond the ones every repository has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PathRules {
    /// Reject names Windows cannot create (`CON`, `aux.txt`, `a:b`, …).
    pub windows_safe: bool,
}

/// Validate `cv`: the content-free limits from
/// [`rill_types::limits`], symlink targets stored as blobs, and `rules`.
/// Case collisions are checked when the tree is built.
pub async fn validate_version(
    objects: &dyn ObjectStore,
    cv: &ChangeVersion,
    rules: PathRules,
) -> Result<()> {
    check_change(cv)?;
    for (path, change) in &cv.file_changes {
        if rules.windows_safe {
            let touched = matches!(change, FileChange::Set { .. } | FileChange::Conflict(_));
            if let Some(bad) = touched
                .then(|| path.as_str().split('/').find(|c| is_windows_reserved(c)))
                .flatten()
            {
                return Err(Error::InvalidChange(format!(
                    "{path}: {bad:?} cannot exist on Windows"
                )));
            }
        }
        if let FileChange::Set {
            content,
            kind: FileKind::Symlink,
            ..
        } = change
        {
            if !matches!(content, ContentRef::Inline { .. }) {
                let target = read_content(objects, content).await?;
                check_symlink_target(path, &target)?;
            }
        }
    }
    Ok(())
}
