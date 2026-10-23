//! What a reviewing agent sees and what it says.

use std::time::Duration;

use bytes::Bytes;
use rill_core::{BlameLine, Hunk};
use rill_types::objects::review::CommentCategory;
use rill_types::{ChangeId, ChangeVersion, ChangeVersionHash, Intent, RepoPathBuf};

/// One file's change as a reviewer sees it.
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// The path touched.
    pub path: RepoPathBuf,
    /// Content before; `None` when the file was added or is binary or
    /// oversized.
    pub before: Option<Bytes>,
    /// Content after; `None` when the file was deleted or is binary or
    /// oversized.
    pub after: Option<Bytes>,
    /// Line hunks between the two, empty when either side is unavailable.
    pub hunks: Vec<Hunk>,
}

/// Blame of one line that the change touches, taken from the parent version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TouchedLine {
    /// The file.
    pub path: RepoPathBuf,
    /// One-based line number in the parent version.
    pub line_no: u32,
    /// Who last wrote the line.
    pub blame: BlameLine,
}

/// Limits a reviewer is expected to honor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Wall-clock time allowed for one reviewer.
    pub timeout: Duration,
    /// Rough input token budget; diffs beyond it are dropped from the
    /// context and `truncated` is set.
    pub max_input_tokens: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            timeout: Duration::from_secs(120),
            max_input_tokens: 100_000,
        }
    }
}

/// Everything a reviewer is given.
#[derive(Debug, Clone)]
pub struct ReviewContext {
    /// The change under review.
    pub change_id: ChangeId,
    /// Hash of the version reviewed.
    pub version: ChangeVersionHash,
    /// The version object: message, author, provenance.
    pub change: ChangeVersion,
    /// Per-file diffs against the first parent.
    pub diffs: Vec<FileDiff>,
    /// The linked intent, when it exists.
    pub intent: Option<Intent>,
    /// Blame of the parent-side lines the change modifies or removes.
    pub blame_for_touched_lines: Vec<TouchedLine>,
    /// The stack the change belongs to (just itself when standalone).
    pub stack: Vec<ChangeId>,
    /// Limits in force.
    pub budget: Budget,
    /// Whether diffs were dropped to fit the token budget.
    pub truncated: bool,
}

/// One observation a reviewer makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file concerned; `None` for a change-level finding.
    pub path: Option<RepoPathBuf>,
    /// One-based inclusive line range in the new version; `None` for a
    /// whole-file or change-level finding.
    pub lines: Option<(u32, u32)>,
    /// What kind of finding this is.
    pub category: CommentCategory,
    /// Confidence 0-100.
    pub confidence: u8,
    /// The comment body.
    pub body: String,
}

impl Finding {
    /// A change-level finding.
    pub fn change(category: CommentCategory, confidence: u8, body: impl Into<String>) -> Self {
        Finding {
            path: None,
            lines: None,
            category,
            confidence,
            body: body.into(),
        }
    }

    /// A finding on a line range of a file.
    pub fn at(
        path: RepoPathBuf,
        start: u32,
        end: u32,
        category: CommentCategory,
        confidence: u8,
        body: impl Into<String>,
    ) -> Self {
        Finding {
            path: Some(path),
            lines: Some((start, end)),
            category,
            confidence,
            body: body.into(),
        }
    }

    /// A whole-file finding.
    pub fn file(
        path: RepoPathBuf,
        category: CommentCategory,
        confidence: u8,
        body: impl Into<String>,
    ) -> Self {
        Finding {
            path: Some(path),
            lines: None,
            category,
            confidence,
            body: body.into(),
        }
    }
}
