//! Structured diff output.

use bytes::Bytes;

/// Classification of one line within a hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineTag {
    /// Present on both sides.
    Context,
    /// Present only in the old content.
    Removed,
    /// Present only in the new content.
    Added,
}

/// One line of a hunk, terminator included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub tag: LineTag,
    pub text: Bytes,
}

impl DiffLine {
    /// Build a line from borrowed bytes.
    pub fn new(tag: LineTag, text: &[u8]) -> Self {
        DiffLine {
            tag,
            text: Bytes::copy_from_slice(text),
        }
    }
}

/// A contiguous region of change with surrounding context.
///
/// `old_start` and `new_start` are 1-based like `diff -u`; `old_len` and
/// `new_len` count the lines of each side covered by the hunk (context plus
/// removed, context plus added).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    pub lines: Vec<DiffLine>,
}

impl Hunk {
    /// Number of removed lines.
    pub fn removed(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.tag == LineTag::Removed)
            .count()
    }

    /// Number of added lines.
    pub fn added(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.tag == LineTag::Added)
            .count()
    }
}
