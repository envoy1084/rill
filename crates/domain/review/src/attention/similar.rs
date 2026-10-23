//! Batch review: changes under one intent that make the same edit (a
//! codemod applied file by file) grouped by a signature of their diff, so
//! a reviewer reads one and decides for all.

use std::collections::BTreeMap;

use rill_core::LineTag;
use rill_types::{ChangeId, ChangeVersionHash, Intent, ReviewStatus};
use serde::{Deserialize, Serialize};

use crate::agents::Budget;
use crate::error::Result;
use crate::service::ReviewService;

/// Changes that make the same edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimilarGroup {
    /// The shared diff signature.
    pub signature: String,
    /// The changes, with the version each was grouped at.
    pub changes: Vec<(ChangeId, ChangeVersionHash)>,
}

impl ReviewService {
    /// A signature of what `version` changes, blind to where: the removed
    /// and added lines of every file, trimmed and sorted. Two changes
    /// applying one edit to different files share it.
    pub async fn patch_signature(
        &self,
        change: &ChangeId,
        version: ChangeVersionHash,
    ) -> Result<String> {
        let ctx = self
            .review_context(
                change,
                version,
                Budget {
                    max_input_tokens: usize::MAX,
                    ..Budget::default()
                },
            )
            .await?;
        let mut lines: Vec<String> = Vec::new();
        for d in &ctx.diffs {
            for h in &d.hunks {
                for l in &h.lines {
                    let sign = match l.tag {
                        LineTag::Added => '+',
                        LineTag::Removed => '-',
                        LineTag::Context => continue,
                    };
                    lines.push(format!("{sign}{}", String::from_utf8_lossy(&l.text).trim()));
                }
            }
            if d.hunks.is_empty() {
                lines.push(format!(
                    "={}:{}",
                    d.before.is_some(),
                    d.after.as_ref().map_or(0, |a| a.len())
                ));
            }
        }
        lines.sort();
        let h = blake3::hash(lines.join("\n").as_bytes());
        Ok(h.to_hex()[..16].to_string())
    }

    /// The intent's changes in review, grouped by signature, largest
    /// groups first.
    pub async fn similar_changes(&self, intent: &Intent) -> Result<Vec<SimilarGroup>> {
        let mut groups: BTreeMap<String, Vec<(ChangeId, ChangeVersionHash)>> = BTreeMap::new();
        for change in &intent.changes {
            let Some(state) = self.repo.get_review(change).await? else {
                continue;
            };
            if !matches!(state.status, ReviewStatus::Open | ReviewStatus::Landable) {
                continue;
            }
            let Some(version) = state.current_version() else {
                continue;
            };
            let signature = self.patch_signature(change, version).await?;
            groups
                .entry(signature)
                .or_default()
                .push((*change, version));
        }
        let mut out: Vec<SimilarGroup> = groups
            .into_iter()
            .map(|(signature, changes)| SimilarGroup { signature, changes })
            .collect();
        out.sort_by_key(|g| std::cmp::Reverse(g.changes.len()));
        Ok(out)
    }
}
