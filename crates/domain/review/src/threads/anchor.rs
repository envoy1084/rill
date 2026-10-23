//! Line anchors that survive new versions.
//!
//! A `Lines` anchor carries a hash of the lines around its range. When a
//! new version arrives the same context is searched for in the new file
//! content, nearest to the original position first, and the anchor moves
//! with it, following a rename the new version records as a copy. An
//! anchor whose context no longer exists is left pointing at the version
//! it was last placed on and marked outdated.

use rill_core::diff::{line_content, split_lines};
use rill_types::objects::change::FileChange;
use rill_types::{ChangeVersionHash, RepoPathBuf, ReviewState, ThreadAnchor};

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
    /// Move every file-anchored thread onto `new_version`: to the same path,
    /// or to where the version copied that path (a rename), where its
    /// context can be found. A thread that cannot follow keeps its old
    /// anchor and is marked outdated.
    pub async fn reanchor(
        &self,
        state: &mut ReviewState,
        new_version: ChangeVersionHash,
    ) -> Result<()> {
        let root = self.root_of(&new_version).await?;
        let cv = self.derive.change(&new_version).await?;
        let moved_to = |path: &RepoPathBuf| -> Option<RepoPathBuf> {
            cv.file_changes
                .iter()
                .find_map(|(to, change)| match change {
                    FileChange::Set {
                        copy_from: Some(src),
                        ..
                    } if src.path == *path && to != path => Some(to.clone()),
                    _ => None,
                })
        };
        for thread in &mut state.threads {
            match &thread.anchor {
                ThreadAnchor::Lines {
                    version,
                    path,
                    start,
                    end,
                    context_hash,
                } if *version != new_version => {
                    let mut found = None;
                    for candidate in std::iter::once(path.clone()).chain(moved_to(path)) {
                        if let Some(content) = self.file_at(root, &candidate).await? {
                            if let Some(range) = find_context(&content, *start, *end, context_hash)
                            {
                                found = Some((candidate, range));
                                break;
                            }
                        }
                    }
                    match found {
                        Some((path, (s, e))) => {
                            thread.anchor = ThreadAnchor::Lines {
                                version: new_version,
                                path,
                                start: s,
                                end: e,
                                context_hash: context_hash.clone(),
                            };
                            thread.outdated = false;
                        }
                        None => thread.outdated = true,
                    }
                }
                ThreadAnchor::File { version, path } if *version != new_version => {
                    let mut found = None;
                    for candidate in std::iter::once(path.clone()).chain(moved_to(path)) {
                        if self.file_at(root, &candidate).await?.is_some() {
                            found = Some(candidate);
                            break;
                        }
                    }
                    match found {
                        Some(path) => {
                            thread.anchor = ThreadAnchor::File {
                                version: new_version,
                                path,
                            };
                            thread.outdated = false;
                        }
                        None => thread.outdated = true,
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
