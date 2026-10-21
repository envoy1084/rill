//! Merging two change versions into a new one with two parents.

use std::collections::BTreeSet;
use std::sync::Arc;

use rill_types::objects::change::FileState;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, FileChange, Merge, Principal, Provenance, RepoPath,
    RepoPathBuf, Timestamp, TreeHash,
};

use super::base::MergeBase;
use super::content::Structural;
use super::state::{change_terms, merge_file_state_with, Merged};
use crate::derive::{DeriveManager, RootTree};
use crate::diff::{diff_trees, file_state};
use crate::error::Result;
use crate::graph::CommitGraph;

/// Metadata for a merge change.
#[derive(Debug, Clone)]
pub struct MergeChangeOptions {
    pub author: Principal,
    pub message: String,
    pub created_at: Timestamp,
    /// Identity of the new change; a fresh id when `None`.
    pub change_id: Option<ChangeId>,
    pub provenance: Provenance,
}

impl MergeChangeOptions {
    /// Options with a message and author, created now.
    pub fn new(author: Principal, message: impl Into<String>) -> Self {
        MergeChangeOptions {
            author,
            message: message.into(),
            created_at: Timestamp::now(),
            change_id: None,
            provenance: Provenance::default(),
        }
    }
}

/// Merge `right` into `left`, producing a version whose first parent is
/// `left`.
///
/// Every path that `right` changed since the merge base is merged
/// three-way against `left`; paths only `left` changed are already in the
/// first parent and need no entry. A tip version that records a `Conflict`
/// at a path contributes its terms rather than the first-add state its
/// tree holds, so conflicts combine instead of being silently picked.
/// Conflicts in the result are stored as data.
pub async fn merge_changes(
    derive: &Arc<DeriveManager>,
    graph: &CommitGraph,
    left: &ChangeVersionHash,
    right: &ChangeVersionHash,
    options: MergeChangeOptions,
) -> Result<ChangeVersion> {
    let base = MergeBase::of(derive, graph, left, right).await?;
    let left_root = derive.get::<RootTree>(left).await?;
    let right_root = derive.get::<RootTree>(right).await?;
    let left_cv = derive.change(left).await?;
    let right_cv = derive.change(right).await?;
    let trees = derive.trees();

    let base_roots: Vec<Option<TreeHash>> = match &base {
        Some(base) => base.roots().into_iter().map(Some).collect(),
        None => vec![None],
    };
    let mut paths: BTreeSet<RepoPathBuf> = BTreeSet::new();
    for base_root in base_roots {
        let changed = diff_trees(trees, base_root, Some(right_root)).await?;
        paths.extend(changed.into_iter().map(|e| e.path));
    }
    paths.extend(left_cv.conflicted_paths().cloned());
    paths.extend(right_cv.conflicted_paths().cloned());

    let mut file_changes = std::collections::BTreeMap::new();
    let mut structural = BTreeSet::new();
    for path in paths {
        let base_terms = match &base {
            Some(base) => base.terms(derive, &path).await?,
            None => Merge::resolved(None),
        };
        let left_state = file_state(trees, Some(left_root), &path).await?;
        let left_terms = tip_terms(&left_cv, &path, left_state.clone());
        let right_state = file_state(trees, Some(right_root), &path).await?;
        let right_terms = tip_terms(&right_cv, &path, right_state);
        let merged = merge_terms(derive, &path, base_terms, left_terms, right_terms).await?;
        if is_same_as(&merged.change, &left_state) {
            continue;
        }
        if merged.structural {
            structural.insert(path.clone());
        }
        file_changes.insert(path, merged.change);
    }

    let mut cv = ChangeVersion {
        change_id: options.change_id.unwrap_or_default(),
        parents: vec![*left, *right],
        predecessor: None,
        subtree_changes: Default::default(),
        file_changes,
        author: options.author,
        provenance: options.provenance,
        intent: None,
        message: options.message,
        created_at: options.created_at,
        extra: Default::default(),
    };
    if let Some(merger) = derive.merger() {
        super::content::record_structural(&mut cv, merger.name(), &structural);
    }
    Ok(cv)
}

/// Terms of a path on one side: the tip's conflict terms if it records
/// one, else the tree state.
pub(crate) fn tip_terms(
    tip: &ChangeVersion,
    path: &RepoPath,
    state: FileState,
) -> Merge<FileState> {
    match tip.file_changes.get(path) {
        Some(change @ FileChange::Conflict(_)) => change_terms(change),
        _ => Merge::resolved(state),
    }
}

/// Three-way merge of a path: `left + (right - base)` over the algebra,
/// then by content.
pub(crate) async fn merge_path(
    derive: &Arc<DeriveManager>,
    path: &RepoPath,
    base: FileState,
    left: Merge<FileState>,
    right: Merge<FileState>,
) -> Result<Merged> {
    merge_terms(derive, path, Merge::resolved(base), left, right).await
}

/// [`merge_path`] with a base that may itself be conflicted (a virtual
/// base from a criss-cross history).
async fn merge_terms(
    derive: &Arc<DeriveManager>,
    path: &RepoPath,
    base: Merge<FileState>,
    left: Merge<FileState>,
    right: Merge<FileState>,
) -> Result<Merged> {
    let terms = Merge::three_way(base, left, right).flatten();
    let objects = derive.repo().objects();
    let structural = derive.merger().map(|merger| Structural { merger, path });
    merge_file_state_with(terms, objects, objects.as_ref(), structural).await
}

/// True when `change` leaves the path exactly as `state`.
pub(crate) fn is_same_as(change: &FileChange, state: &FileState) -> bool {
    match change.resolved_state() {
        Some(resolved) => &resolved == state,
        None => false,
    }
}

/// Root tree of a version or `None` for "no parent".
pub(crate) async fn root_of(
    derive: &Arc<DeriveManager>,
    version: Option<ChangeVersionHash>,
) -> Result<Option<TreeHash>> {
    match version {
        Some(v) => Ok(Some(derive.get::<RootTree>(&v).await?)),
        None => Ok(None),
    }
}
