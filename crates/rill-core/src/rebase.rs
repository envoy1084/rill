//! Rebasing a change version onto a new parent.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rill_types::{
    ChangeVersion, ChangeVersionHash, Merge, RepoPath, RepoPathBuf, Timestamp, TreeHash,
};

use crate::derive::{DeriveManager, RootTree};
use crate::diff::{diff_trees, file_state};
use crate::error::Result;
use crate::merge::change::{is_same_as, merge_path, root_of, tip_terms};
use crate::tree::TreeOps;

/// Rebase `version` onto `new_parent`, producing the next version of the
/// same change (predecessor = `version`).
///
/// The change's effect is the tree diff from its old first parent to
/// itself (plus any paths it records as conflicts). Each affected path is
/// merged three-way with base = old parent, left = new parent and right =
/// the old version, so the new version carries the same intent on top of
/// the new base; conflicts become `Conflict` entries. Paths whose merged
/// state equals the new parent's are dropped.
///
/// A subtree change is kept as-is only when the new parent left that
/// directory exactly as the old parent had it. Otherwise replaying it would
/// silently overwrite the new parent's work there, so it is expanded into
/// the per-file changes it made and those are merged like any other path.
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

    let mut subtree_changes = BTreeMap::new();
    for (dir, change) in &cv.subtree_changes {
        let before = dir_at(trees, old_parent_root, dir).await?;
        let after = dir_at(trees, Some(new_parent_root), dir).await?;
        if before == after {
            subtree_changes.insert(dir.clone(), change.clone());
        }
    }
    paths.retain(|p| {
        !subtree_changes
            .keys()
            .any(|s: &RepoPathBuf| p.starts_with(s) && p != s)
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
        subtree_changes,
        file_changes,
        author: cv.author,
        provenance: cv.provenance,
        intent: cv.intent,
        message: cv.message,
        created_at,
        extra: cv.extra,
    })
}

async fn dir_at(
    trees: &TreeOps,
    root: Option<TreeHash>,
    path: &RepoPath,
) -> Result<Option<TreeHash>> {
    match root {
        Some(root) => trees.dir_hash(root, path).await,
        None => Ok(None),
    }
}
