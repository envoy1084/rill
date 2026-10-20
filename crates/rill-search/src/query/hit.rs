//! A search result.

use std::ops::Range;

use rill_types::RepoPathBuf;

/// One matching line of one file.
///
/// A line with several matches yields a single hit whose `byte_range` is
/// the first match; the line text is enough to locate the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// File containing the match.
    pub path: RepoPathBuf,
    /// 1-based line number.
    pub line_no: u32,
    /// The matching line without its terminator, lossily decoded.
    pub line: String,
    /// Up to `context_lines` lines preceding the match.
    pub before: Vec<String>,
    /// Up to `context_lines` lines following the match.
    pub after: Vec<String>,
    /// Byte offsets of the first match on the line, within the whole file.
    pub byte_range: Range<usize>,
}
