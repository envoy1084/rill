//! Rebasing a change version onto a new parent.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rill_types::{ChangeVersion, ChangeVersionHash, Merge, RepoPathBuf, Timestamp};

use crate::derive::{DeriveManager, RootTree};
use crate::diff::{diff_trees, file_state};
use crate::error::Result;
use crate::merge::change::{is_same_as, merge_path, root_of, tip_terms};

/// Rebase `version` onto `new_parent`, producing the next version of the
/// same change (predecessor = `version`).
///
/// The change's effect is the tree diff from its old first parent to
/// itself (plus any paths it records as conflicts). Each affected path is
/// merged three-way with base = old parent, left = new parent and right =
/// the old version, so the new version carries the same intent on top of
/// the new base; conflicts become `Conflict` entries. Paths whose merged
/// state equals the new parent's are dropped. Subtree changes are copied
/// verbatim and the paths beneath them are left to those entries.
pub async fn rebase(
    derive: &Arc<DeriveManager>,
    version: &ChangeVersionHash,
    new_parent: &ChangeVersionHash,
    created_at: Timestamp,
) -> Result<ChangeVersion> {
    let cv = derive.change(version).await?;
    let old_parent = cv.first_parent();
    let old_parent_root = root_of(derive, old_parent).await?;
    let old_root = derive.get::<RootTree>(version).await?;
    let new_parent_root = derive.get::<RootTree>(new_parent).await?;
    let trees = derive.trees();

    let mut paths: BTreeSet<RepoPathBuf> = diff_trees(trees, old_parent_root, Some(old_root))
        .await?
        .into_iter()
        .map(|e| e.path)
        .collect();
    paths.extend(cv.conflicted_paths().cloned());
    paths.retain(|p| {
        !cv.subtree_changes
            .keys()
            .any(|s| p.starts_with(s) && p != s)
    });

    let mut file_changes = BTreeMap::new();
    for path in paths {
        let base = file_state(trees, old_parent_root, &path).await?;
        let left = file_state(trees, Some(new_parent_root), &path).await?;
        let right = tip_terms(&cv, &path, file_state(trees, Some(old_root), &path).await?);
        let change = merge_path(derive, base, Merge::resolved(left.clone()), right).await?;
        if is_same_as(&change, &left) {
            continue;
        }
        file_changes.insert(path, change);
    }

    let mut parents = vec![*new_parent];
    parents.extend(cv.parents.iter().skip(1).filter(|p| *p != new_parent));
    Ok(ChangeVersion {
        change_id: cv.change_id,
        parents,
        predecessor: Some(*version),
        subtree_changes: cv.subtree_changes,
        file_changes,
        author: cv.author,
        provenance: cv.provenance,
        intent: cv.intent,
        message: cv.message,
        created_at,
        extra: cv.extra,
    })
}
