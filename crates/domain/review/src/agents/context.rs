//! Assembling a [`ReviewContext`] for a change.

use rill_core::{diff_lines, diff_trees, LineTag};

use super::reviewer::{Budget, FileDiff, ReviewContext, TouchedLine};
use crate::error::{Error, Result};
use crate::service::ReviewService;
use rill_types::{ChangeId, ChangeVersionHash};

/// Rough bytes-per-token used to fit diffs into a token budget.
const BYTES_PER_TOKEN: usize = 4;

impl ReviewService {
    /// Build the context a reviewer sees for `version` of the change.
    pub async fn review_context(
        &self,
        change_id: &ChangeId,
        version: ChangeVersionHash,
        budget: Budget,
    ) -> Result<ReviewContext> {
        let state = self.get(change_id).await?;
        if !state.versions.contains(&version) {
            return Err(Error::WrongChange {
                version,
                expected: *change_id,
                actual: self.derive.change(&version).await?.change_id,
            });
        }
        let change = self.derive.change(&version).await?;
        let root = self.root_of(&version).await?;
        let parent = change.first_parent();
        let parent_root = match parent {
            Some(p) => Some(self.root_of(&p).await?),
            None => None,
        };

        let mut diffs = Vec::new();
        let mut blame_for_touched_lines = Vec::new();
        for entry in diff_trees(self.derive.trees(), parent_root, Some(root)).await? {
            let before = self.text_of(entry.before.as_ref().map(|(c, _)| c)).await?;
            let after = self.text_of(entry.after.as_ref().map(|(c, _)| c)).await?;
            let hunks = match (&before, &after) {
                (Some(b), Some(a)) => diff_lines(b, a),
                _ => Vec::new(),
            };
            if let (Some(parent), true) = (parent, before.is_some()) {
                // Blame is best-effort: a binary or vanished file yields
                // nothing rather than failing the whole review.
                if let Ok(blame) = self.derive.blame(&parent, &entry.path).await {
                    for line_no in removed_line_numbers(&hunks) {
                        if let Some(line) = blame.get(line_no as usize - 1) {
                            blame_for_touched_lines.push(TouchedLine {
                                path: entry.path.clone(),
                                line_no,
                                blame: line.clone(),
                            });
                        }
                    }
                }
            }
            diffs.push(FileDiff {
                path: entry.path,
                before,
                after,
                hunks,
            });
        }
        let truncated = fit_to_budget(&mut diffs, budget.max_input_tokens);

        let intent = match change.intent {
            Some(id) => self.repo.get_intent(&id).await?,
            None => None,
        };
        let stack = if state.stack.is_empty() {
            vec![*change_id]
        } else {
            state.stack.clone()
        };
        Ok(ReviewContext {
            change_id: *change_id,
            version,
            change,
            diffs,
            intent,
            blame_for_touched_lines,
            stack,
            budget,
            truncated,
        })
    }
}

/// Old-side line numbers of every removed line.
fn removed_line_numbers(hunks: &[rill_core::Hunk]) -> Vec<u32> {
    let mut out = Vec::new();
    for h in hunks {
        let mut old_no = h.old_start;
        for line in &h.lines {
            match line.tag {
                LineTag::Removed => {
                    out.push(old_no);
                    old_no += 1;
                }
                LineTag::Context => old_no += 1,
                LineTag::Added => {}
            }
        }
    }
    out
}

/// Drop content from the largest diffs until the estimated token count
/// fits. Paths are kept so reviewers still know what was touched.
fn fit_to_budget(diffs: &mut [FileDiff], max_tokens: usize) -> bool {
    let size = |d: &FileDiff| {
        d.before.as_ref().map_or(0, |b| b.len()) + d.after.as_ref().map_or(0, |a| a.len())
    };
    let mut total: usize = diffs.iter().map(size).sum::<usize>() / BYTES_PER_TOKEN;
    if total <= max_tokens {
        return false;
    }
    let mut order: Vec<usize> = (0..diffs.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(size(&diffs[i])));
    for i in order {
        if total <= max_tokens {
            break;
        }
        total -= size(&diffs[i]) / BYTES_PER_TOKEN;
        diffs[i].before = None;
        diffs[i].after = None;
        diffs[i].hunks.clear();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use rill_types::RepoPathBuf;

    #[test]
    fn budget_drops_largest_first() {
        let mk = |p: &str, n: usize| FileDiff {
            path: RepoPathBuf::new(p).unwrap(),
            before: None,
            after: Some(Bytes::from(vec![b'x'; n])),
            hunks: Vec::new(),
        };
        let mut diffs = vec![mk("small", 40), mk("big", 4000), mk("mid", 400)];
        assert!(fit_to_budget(&mut diffs, 200));
        assert!(diffs[1].after.is_none());
        assert!(diffs[0].after.is_some());
        assert!(diffs[2].after.is_some());
        assert!(!fit_to_budget(&mut diffs, 200));
    }

    #[test]
    fn removed_numbers_follow_old_side() {
        let hunks = diff_lines(b"a\nb\nc\nd\n", b"a\nc\nD\n");
        assert_eq!(removed_line_numbers(&hunks), vec![2, 4]);
    }
}
