//! Line anchors that survive new versions.
//!
//! A `Lines` anchor carries a hash of the lines around its range. When a
//! new version arrives the same context is searched for in the new file
//! content, nearest to the original position first, and the anchor moves
//! with it. An anchor whose context no longer exists is left pointing at
//! the version it was made on.

use rill_core::diff::{line_content, split_lines};
use rill_types::{ChangeVersionHash, ReviewState, ThreadAnchor};

use crate::error::Result;
use crate::service::ReviewService;

/// Lines of context hashed on each side of an anchored range.
pub const CONTEXT_LINES: u32 = 1;

/// Hash of the lines `start - CONTEXT_LINES ..= end + CONTEXT_LINES`
/// (1-based, clipped to the file) with terminators normalized away.
pub fn context_hash(content: &[u8], start: u32, end: u32) -> String {
    let lines: Vec<&[u8]> = split_lines(content).map(line_content).collect();
    hash_window(&lines, start, end)
}

fn hash_window(lines: &[&[u8]], start: u32, end: u32) -> String {
    let (lo, hi) = window(lines.len(), start, end);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rill:anchor:");
    for line in &lines[lo..hi] {
        hasher.update(line);
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize().as_bytes())
}

/// Zero-based half-open window of lines hashed for a 1-based range.
fn window(len: usize, start: u32, end: u32) -> (usize, usize) {
    let start = start.max(1) as usize;
    let end = (end.max(start as u32)) as usize;
    let lo = start.saturating_sub(1 + CONTEXT_LINES as usize).min(len);
    let hi = (end + CONTEXT_LINES as usize).min(len);
    (lo, hi.max(lo))
}

/// Find the range of the same length whose context hashes to `hash`,
/// preferring the one closest to `start`. Returns the new `(start, end)`.
pub fn find_context(content: &[u8], start: u32, end: u32, hash: &str) -> Option<(u32, u32)> {
    let lines: Vec<&[u8]> = split_lines(content).map(line_content).collect();
    if lines.is_empty() {
        return None;
    }
    let len = end.saturating_sub(start);
    let last_start = (lines.len() as u32).saturating_sub(len).max(1);
    // Spiral outward from the original position so the nearest match wins.
    let mut candidates: Vec<u32> = (1..=last_start).collect();
    candidates.sort_by_key(|s| (s.abs_diff(start), *s));
    candidates
        .into_iter()
        .find(|&s| hash_window(&lines, s, s + len) == hash)
        .map(|s| (s, s + len))
}

impl ReviewService {
    /// Move every file-anchored thread onto `new_version` where its context
    /// can be found. Threads whose anchor cannot be re-found keep their old
    /// anchor.
    pub async fn reanchor(
        &self,
        state: &mut ReviewState,
        new_version: ChangeVersionHash,
    ) -> Result<()> {
        let root = self.root_of(&new_version).await?;
        for thread in &mut state.threads {
            match &thread.anchor {
                ThreadAnchor::Lines {
                    version,
                    path,
                    start,
                    end,
                    context_hash,
                } if *version != new_version => {
                    let Some(content) = self.file_at(root, path).await? else {
                        continue;
                    };
                    if let Some((s, e)) = find_context(&content, *start, *end, context_hash) {
                        thread.anchor = ThreadAnchor::Lines {
                            version: new_version,
                            path: path.clone(),
                            start: s,
                            end: e,
                            context_hash: context_hash.clone(),
                        };
                    }
                }
                ThreadAnchor::File { version, path } if *version != new_version => {
                    let exists = self.file_at(root, path).await?.is_some();
                    if exists {
                        thread.anchor = ThreadAnchor::File {
                            version: new_version,
                            path: path.clone(),
                        };
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &[u8] = b"a\nb\nc\nd\ne\n";

    #[test]
    fn hash_covers_surrounding_lines() {
        let h = context_hash(TEXT, 3, 3);
        // Window is b, c, d; changing e does not affect it.
        assert_eq!(h, context_hash(b"a\nb\nc\nd\nX\n", 3, 3));
        assert_ne!(h, context_hash(b"a\nb\nc\nX\ne\n", 3, 3));
        assert_ne!(h, context_hash(TEXT, 2, 2));
        // Clipping at the edges.
        assert_eq!(context_hash(TEXT, 1, 1), context_hash(b"a\nb\nzzz\n", 1, 1));
    }

    #[test]
    fn finds_shifted_context() {
        let h = context_hash(TEXT, 3, 4);
        let shifted = b"new\nnew\na\nb\nc\nd\ne\n";
        assert_eq!(find_context(shifted, 3, 4, &h), Some((5, 6)));
        assert_eq!(find_context(b"x\ny\n", 3, 4, &h), None);
        assert_eq!(find_context(TEXT, 3, 4, &h), Some((3, 4)));
    }

    #[test]
    fn nearest_match_wins() {
        let repeated = b"a\nb\nc\nd\ne\na\nb\nc\nd\ne\n";
        let h = context_hash(TEXT, 3, 3);
        assert_eq!(find_context(repeated, 7, 7, &h), Some((8, 8)));
        assert_eq!(find_context(repeated, 4, 4, &h), Some((3, 3)));
    }
}
