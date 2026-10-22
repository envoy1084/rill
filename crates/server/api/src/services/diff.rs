//! Building `FileDiff` records from two root trees.

use rill_core::diff::{diff_lines_with_context, Hunk, LineTag, DEFAULT_CONTEXT};
use rill_core::{diff_trees, is_binary, read_content, to_unified};
use rill_policy::PathFilter;
use rill_proto::convert::{file_state_to_proto, merge_terms_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersion, FileChange, RepoPathBuf, TreeHash};

use rill_runtime::error::Result;
use rill_runtime::state::RepoContext;

/// Largest file content compared line by line; bigger files are reported
/// as changed without hunks.
pub const MAX_DIFF_BYTES: u64 = 4 * 1024 * 1024;

/// Default number of files in one diff response.
pub const DEFAULT_DIFF_LIMIT: usize = 1000;

/// How to render a diff.
pub struct DiffOptions {
    /// Restrict to these paths or prefixes; empty means everything.
    pub paths: Vec<RepoPathBuf>,
    /// Structured hunks or unified text.
    pub format: pb::DiffFormat,
    /// Context lines around each hunk.
    pub context: usize,
    /// Maximum files to return.
    pub limit: usize,
    /// Paths the caller may see.
    pub filter: PathFilter,
}

impl DiffOptions {
    /// Options from the common request fields.
    pub fn new(
        paths: &[String],
        format: buffa::EnumValue<pb::DiffFormat>,
        context_lines: u32,
        limit: u32,
        filter: PathFilter,
    ) -> Result<Self> {
        let paths = paths
            .iter()
            .map(RepoPathBuf::new)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let format = match format.as_known() {
            Some(pb::DiffFormat::Unified) => pb::DiffFormat::Unified,
            _ => pb::DiffFormat::Structured,
        };
        Ok(DiffOptions {
            paths,
            format,
            context: if context_lines == 0 {
                DEFAULT_CONTEXT
            } else {
                context_lines as usize
            },
            limit: if limit == 0 {
                DEFAULT_DIFF_LIMIT
            } else {
                limit as usize
            },
            filter,
        })
    }

    fn wants(&self, path: &RepoPathBuf) -> bool {
        self.filter.allows(path)
            && (self.paths.is_empty() || self.paths.iter().any(|p| path.starts_with(p)))
    }
}

/// Diff two roots. `target_version` supplies conflict terms for paths
/// that are unresolved in the target. Returns the diffs and whether the
/// limit cut them short.
pub async fn file_diffs(
    ctx: &RepoContext,
    before: Option<TreeHash>,
    after: Option<TreeHash>,
    target_version: Option<&ChangeVersion>,
    options: &DiffOptions,
) -> Result<(Vec<pb::FileDiff>, bool)> {
    let entries = diff_trees(&ctx.trees, before, after).await?;
    let mut out = Vec::new();
    let mut truncated = false;
    for entry in entries.into_iter().filter(|e| options.wants(&e.path)) {
        if out.len() >= options.limit {
            truncated = true;
            break;
        }
        let conflict = target_version
            .and_then(|cv| cv.file_changes.get(&entry.path))
            .and_then(|c| match c {
                FileChange::Conflict(terms) => Some(merge_terms_to_proto(terms)),
                _ => None,
            });
        let (hunks, unified, binary) = match (&entry.before, &entry.after) {
            (None, None) => (Vec::new(), None, false),
            _ => {
                let old = content_for_diff(ctx, entry.before.as_ref().map(|(c, _)| c)).await?;
                let new = content_for_diff(ctx, entry.after.as_ref().map(|(c, _)| c)).await?;
                match (old, new) {
                    (Some(old), Some(new)) if !is_binary(&old) && !is_binary(&new) => {
                        let hunks = diff_lines_with_context(&old, &new, options.context);
                        render(&entry.path, hunks, options)
                    }
                    _ => (Vec::new(), None, true),
                }
            }
        };
        out.push(pb::FileDiff {
            path: entry.path.to_string(),
            before: file_state_to_proto(&entry.before).into(),
            after: file_state_to_proto(&entry.after).into(),
            hunks,
            unified,
            binary,
            conflict: conflict.into(),
        });
    }
    Ok((out, truncated))
}

/// Content bytes for diffing; `None` for an absent file is treated as
/// empty so added and deleted files still get hunks. Files over the size
/// limit are reported as binary (no hunks).
async fn content_for_diff(
    ctx: &RepoContext,
    content: Option<&rill_types::ContentRef>,
) -> Result<Option<bytes::Bytes>> {
    match content {
        None => Ok(Some(bytes::Bytes::new())),
        Some(c) if c.size() > MAX_DIFF_BYTES => Ok(None),
        Some(c) => Ok(Some(read_content(ctx.repo.objects().as_ref(), c).await?)),
    }
}

fn render(
    path: &RepoPathBuf,
    hunks: Vec<Hunk>,
    options: &DiffOptions,
) -> (Vec<pb::DiffHunk>, Option<String>, bool) {
    match options.format {
        pb::DiffFormat::Unified => (
            Vec::new(),
            Some(String::from_utf8_lossy(&to_unified(path.as_str(), &hunks)).into_owned()),
            false,
        ),
        _ => (hunks.iter().map(hunk_proto).collect(), None, false),
    }
}

/// Convert one hunk to its wire form.
pub fn hunk_proto(hunk: &Hunk) -> pb::DiffHunk {
    pb::DiffHunk {
        before_start: hunk.old_start,
        before_len: hunk.old_len,
        after_start: hunk.new_start,
        after_len: hunk.new_len,
        lines: hunk
            .lines
            .iter()
            .map(|l| pb::DiffLine {
                op: (match l.tag {
                    LineTag::Context => pb::diff_line::Op::Context,
                    LineTag::Removed => pb::diff_line::Op::Delete,
                    LineTag::Added => pb::diff_line::Op::Insert,
                })
                .into(),
                text: l.text.clone(),
            })
            .collect(),
    }
}
