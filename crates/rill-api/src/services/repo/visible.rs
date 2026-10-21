//! History as a path-limited reader sees it: versions reduced to the
//! changes under readable paths, and hidden when they change nothing
//! readable (their message and author would describe hidden work).

use rill_policy::PathFilter;
use rill_types::{ChangeInfo, ChangeVersion, ChangeVersionHash, SubtreeChange};

use crate::error::Result;
use crate::state::RepoContext;

/// `cv` with only the changes `filter` allows; copies from unreadable
/// sources are dropped too.
pub fn version(cv: ChangeVersion, filter: &PathFilter) -> ChangeVersion {
    if filter.is_all() {
        return cv;
    }
    let mut cv = cv;
    cv.file_changes.retain(|path, change| {
        filter.allows(path)
            && match change {
                rill_types::FileChange::Set {
                    copy_from: Some(source),
                    ..
                } => filter.allows(&source.path),
                _ => true,
            }
    });
    cv.subtree_changes.retain(|path, change| {
        filter.allows(path)
            && match change {
                SubtreeChange::Copy { from_path, .. } => filter.allows(from_path),
                SubtreeChange::Delete => true,
            }
    });
    cv
}

/// Whether the version changes anything `filter` allows.
pub fn touches(cv: &ChangeVersion, filter: &PathFilter) -> bool {
    filter.is_all()
        || cv.file_changes.keys().any(|p| filter.allows(p))
        || cv.subtree_changes.keys().any(|p| filter.allows(p))
}

/// The version's info as `filter` sees it, or `None` when it is hidden.
/// `files_changed` counts only readable files.
pub async fn info(
    ctx: &RepoContext,
    filter: &PathFilter,
    info: ChangeInfo,
) -> Result<Option<ChangeInfo>> {
    if filter.is_all() {
        return Ok(Some(info));
    }
    let cv = ctx.derive.change(&info.hash).await?;
    if !touches(&cv, filter) {
        return Ok(None);
    }
    let shown = version(cv, filter);
    Ok(Some(ChangeInfo {
        files_changed: shown.file_changes.len() as u64,
        ..info
    }))
}

/// The info of `hash` as `filter` sees it.
pub async fn info_of(
    ctx: &RepoContext,
    filter: &PathFilter,
    hash: &ChangeVersionHash,
) -> Result<Option<ChangeInfo>> {
    let found = ctx
        .repo
        .get_change_info(hash)
        .await?
        .ok_or_else(|| crate::error::Error::not_found(format!("change version {hash}")))?;
    info(ctx, filter, found).await
}
