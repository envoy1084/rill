//! Diff results.

use bytes::Bytes;
use rill_proto::convert::{file_state_from_proto, merge_terms_from_proto};
use rill_proto::rill::v1 as pb;
use rill_types::objects::change::FileState;
use rill_types::{Merge, RepoPathBuf};

use crate::error::{Error, Result};

/// How the server renders hunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DiffFormat {
    /// Line-level hunks as data.
    #[default]
    Structured,
    /// Unified diff text per file.
    Unified,
}

impl DiffFormat {
    /// Wire encoding.
    pub fn to_proto(self) -> i32 {
        match self {
            DiffFormat::Structured => pb::DiffFormat::Structured as i32,
            DiffFormat::Unified => pb::DiffFormat::Unified as i32,
        }
    }
}

/// Tuning for a diff request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffOptions {
    /// Restrict to these paths or prefixes; empty means everything.
    pub paths: Vec<RepoPathBuf>,
    /// Hunk rendering.
    pub format: DiffFormat,
    /// Context lines around each hunk.
    pub context_lines: u32,
    /// Maximum files to return; `0` lets the server choose.
    pub limit: u32,
}

impl Default for DiffOptions {
    fn default() -> Self {
        DiffOptions {
            paths: Vec::new(),
            format: DiffFormat::Structured,
            context_lines: 3,
            limit: 0,
        }
    }
}

/// What a diff line does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffOp {
    /// Present on both sides.
    Context,
    /// Present only after.
    Insert,
    /// Present only before.
    Delete,
}

/// One line of a structured diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Line operation.
    pub op: DiffOp,
    /// Line bytes, without a terminator.
    pub text: Bytes,
}

impl TryFrom<pb::DiffLine> for DiffLine {
    type Error = Error;

    fn try_from(l: pb::DiffLine) -> Result<Self> {
        let op = match pb::diff_line::Op::try_from(l.op) {
            Ok(pb::diff_line::Op::Context) => DiffOp::Context,
            Ok(pb::diff_line::Op::Insert) => DiffOp::Insert,
            Ok(pb::diff_line::Op::Delete) => DiffOp::Delete,
            _ => {
                return Err(rill_proto::ConvertError::BadEnum {
                    field: "diff_line.op",
                    value: l.op,
                }
                .into())
            }
        };
        Ok(DiffLine { op, text: l.text })
    }
}

/// A contiguous block of changed lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// One-based first line on the before side.
    pub before_start: u32,
    /// Lines covered on the before side.
    pub before_len: u32,
    /// One-based first line on the after side.
    pub after_start: u32,
    /// Lines covered on the after side.
    pub after_len: u32,
    /// The hunk's lines.
    pub lines: Vec<DiffLine>,
}

impl TryFrom<pb::DiffHunk> for Hunk {
    type Error = Error;

    fn try_from(h: pb::DiffHunk) -> Result<Self> {
        Ok(Hunk {
            before_start: h.before_start,
            before_len: h.before_len,
            after_start: h.after_start,
            after_len: h.after_len,
            lines: h
                .lines
                .into_iter()
                .map(DiffLine::try_from)
                .collect::<Result<_>>()?,
        })
    }
}

/// The difference of one path between two roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// The path.
    pub path: RepoPathBuf,
    /// State on the base side; `None` if absent.
    pub before: FileState,
    /// State on the target side; `None` if absent.
    pub after: FileState,
    /// Structured hunks (empty for `DiffFormat::Unified` or binary files).
    pub hunks: Vec<Hunk>,
    /// Unified text (set for `DiffFormat::Unified`).
    pub unified: Option<String>,
    /// True when either side is binary and no hunks were produced.
    pub binary: bool,
    /// Set when the target side is an unresolved conflict.
    pub conflict: Option<Merge<FileState>>,
}

impl FileDiff {
    /// True when the path was added on the target side.
    pub fn is_added(&self) -> bool {
        self.before.is_none() && self.after.is_some()
    }

    /// True when the path was removed on the target side.
    pub fn is_deleted(&self) -> bool {
        self.before.is_some() && self.after.is_none()
    }
}

impl TryFrom<pb::FileDiff> for FileDiff {
    type Error = Error;

    fn try_from(d: pb::FileDiff) -> Result<Self> {
        Ok(FileDiff {
            path: RepoPathBuf::new(&d.path)?,
            before: d.before.map(file_state_from_proto).transpose()?.flatten(),
            after: d.after.map(file_state_from_proto).transpose()?.flatten(),
            hunks: d
                .hunks
                .into_iter()
                .map(Hunk::try_from)
                .collect::<Result<_>>()?,
            unified: d.unified,
            binary: d.binary,
            conflict: d.conflict.map(merge_terms_from_proto).transpose()?,
        })
    }
}

/// A diff response, with the truncation flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffResult {
    /// Per-file diffs.
    pub files: Vec<FileDiff>,
    /// True when the server stopped at the requested limit.
    pub truncated: bool,
}
