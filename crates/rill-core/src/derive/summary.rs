//! Per-change statistics used by policy risk classification.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use rill_types::{ChangeVersion, FileChange, RepoPath};
use serde::{Deserialize, Serialize};

use super::ctx::DeriveCtx;
use super::root_tree::RootTree;
use super::traits::{Derivable, Schedule};
use crate::diff::file_state;
use crate::error::Result;

/// How a path was affected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryKind {
    Added,
    Modified,
    Deleted,
    Conflicted,
}

/// Counts and shape of a change, cheap enough to compute for every version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub files_added: u32,
    pub files_modified: u32,
    pub files_deleted: u32,
    pub files_conflicted: u32,
    /// Bytes of content added (sum of new sizes of set paths).
    pub bytes_added: u64,
    /// Bytes of content removed (sum of old sizes of changed paths).
    pub bytes_removed: u64,
    /// Files touched per lowercase extension; `""` for none.
    pub languages: BTreeMap<String, u32>,
    /// First path component of every touched path; `""` for root files.
    pub top_level_dirs: BTreeSet<String>,
    /// Number of whole-directory operations.
    pub subtree_changes: u32,
}

impl ChangeSummary {
    /// Total files touched.
    pub fn files_touched(&self) -> u32 {
        self.files_added + self.files_modified + self.files_deleted + self.files_conflicted
    }

    /// Net size change.
    pub fn byte_delta(&self) -> i64 {
        self.bytes_added as i64 - self.bytes_removed as i64
    }
}

#[async_trait]
impl Derivable for ChangeSummary {
    const KIND: &'static str = "summary";
    const SCHEDULE: Schedule = Schedule::Background;
    const VERSION: u8 = 1;
    const NEEDS_PARENTS: bool = false;
    type Value = ChangeSummary;

    async fn derive(
        ctx: &DeriveCtx,
        cv: &ChangeVersion,
        _parents: Vec<ChangeSummary>,
    ) -> Result<ChangeSummary> {
        let parent_root = match cv.first_parent() {
            Some(p) => Some(ctx.manager().get::<RootTree>(&p).await?),
            None => None,
        };
        let mut s = ChangeSummary {
            subtree_changes: cv.subtree_changes.len() as u32,
            ..Default::default()
        };
        for path in cv.subtree_changes.keys() {
            note_path(&mut s, path);
        }
        for (path, change) in &cv.file_changes {
            note_path(&mut s, path);
            let before = file_state(ctx.trees(), parent_root, path).await?;
            let before_size = before.as_ref().map_or(0, |(c, _)| c.size());
            match change {
                FileChange::Set { content, .. } => {
                    if before.is_some() {
                        s.files_modified += 1;
                    } else {
                        s.files_added += 1;
                    }
                    s.bytes_added += content.size();
                    s.bytes_removed += before_size;
                }
                FileChange::Delete => {
                    s.files_deleted += 1;
                    s.bytes_removed += before_size;
                }
                FileChange::Conflict(_) => s.files_conflicted += 1,
            }
            *s.languages
                .entry(path.extension().unwrap_or("").to_ascii_lowercase())
                .or_default() += 1;
        }
        Ok(s)
    }
}

fn note_path(s: &mut ChangeSummary, path: &RepoPath) {
    let top = match path.components().next() {
        Some(first) if path.components().nth(1).is_some() => first,
        _ => "",
    };
    s.top_level_dirs.insert(top.to_string());
}
