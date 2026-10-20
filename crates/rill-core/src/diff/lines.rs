//! Line diff built on `imara-diff`'s Histogram algorithm.

use imara_diff::{Algorithm, Diff, InternedInput};

use super::hunk::{DiffLine, Hunk, LineTag};
use super::tokens::{LineSource, LineTokens};

/// Context lines kept around each change by [`diff_lines`].
pub const DEFAULT_CONTEXT: usize = 3;

/// Raw edit regions `(old_range, new_range)` in line indices.
///
/// Shared with the merge algorithm, which needs the edit script without
/// context grouping.
pub(crate) fn edit_script(
    old: &[&[u8]],
    new: &[&[u8]],
) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let input = InternedInput::new(LineSource(old), LineSource(new));
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    diff.hunks()
        .map(|h| {
            (
                h.before.start as usize..h.before.end as usize,
                h.after.start as usize..h.after.end as usize,
            )
        })
        .collect()
}

/// Diff two byte buffers line by line with [`DEFAULT_CONTEXT`] context.
pub fn diff_lines(old: &[u8], new: &[u8]) -> Vec<Hunk> {
    diff_lines_with_context(old, new, DEFAULT_CONTEXT)
}

/// Diff two byte buffers line by line, grouping edits that are at most
/// `2 * context` lines apart into one hunk.
pub fn diff_lines_with_context(old: &[u8], new: &[u8], context: usize) -> Vec<Hunk> {
    let old_tokens = LineTokens::new(old);
    let new_tokens = LineTokens::new(new);
    let old_lines = old_tokens.lines();
    let new_lines = new_tokens.lines();
    let edits = edit_script(old_lines, new_lines);
    group_hunks(old_lines, new_lines, &edits, context)
}

fn group_hunks(
    old: &[&[u8]],
    new: &[&[u8]],
    edits: &[(std::ops::Range<usize>, std::ops::Range<usize>)],
    context: usize,
) -> Vec<Hunk> {
    let mut hunks = Vec::new();
    let mut i = 0;
    while i < edits.len() {
        // Extend the group while the next edit's leading context overlaps
        // this group's trailing context.
        let mut j = i;
        while j + 1 < edits.len() && edits[j + 1].0.start <= edits[j].0.end + 2 * context {
            j += 1;
        }
        let first = &edits[i];
        let last = &edits[j];
        let old_begin = first.0.start.saturating_sub(context);
        let new_begin = first.1.start.saturating_sub(context);
        let old_end = (last.0.end + context).min(old.len());
        let new_end = (last.1.end + context).min(new.len());

        let mut lines = Vec::new();
        let mut old_pos = old_begin;
        let mut new_pos = new_begin;
        for (old_range, new_range) in &edits[i..=j] {
            for line in &old[old_pos..old_range.start] {
                lines.push(DiffLine::new(LineTag::Context, line));
            }
            for line in &old[old_range.clone()] {
                lines.push(DiffLine::new(LineTag::Removed, line));
            }
            for line in &new[new_range.clone()] {
                lines.push(DiffLine::new(LineTag::Added, line));
            }
            old_pos = old_range.end;
            new_pos = new_range.end;
        }
        let trailing = old_end - old_pos;
        for line in &old[old_pos..old_end] {
            lines.push(DiffLine::new(LineTag::Context, line));
        }
        debug_assert_eq!(new_pos + trailing, new_end);

        hunks.push(Hunk {
            old_start: old_begin as u32 + 1,
            old_len: (old_end - old_begin) as u32,
            new_start: new_begin as u32 + 1,
            new_len: (new_end - new_begin) as u32,
            lines,
        });
        i = j + 1;
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_inputs_have_no_hunks() {
        assert!(diff_lines(b"a\nb\n", b"a\nb\n").is_empty());
        assert!(diff_lines(b"", b"").is_empty());
    }

    #[test]
    fn single_change() {
        let hunks = diff_lines(b"a\nb\nc\n", b"a\nB\nc\n");
        assert_eq!(hunks.len(), 1);
        let h = &hunks[0];
        assert_eq!(
            (h.old_start, h.old_len, h.new_start, h.new_len),
            (1, 3, 1, 3)
        );
        let tags: Vec<_> = h.lines.iter().map(|l| l.tag).collect();
        assert_eq!(
            tags,
            vec![
                LineTag::Context,
                LineTag::Removed,
                LineTag::Added,
                LineTag::Context
            ]
        );
    }

    #[test]
    fn distant_changes_split_into_hunks() {
        let old: Vec<u8> = (0..30)
            .flat_map(|i| format!("{i}\n").into_bytes())
            .collect();
        let mut new = old.clone();
        new.splice(0..2, b"X\n".iter().copied());
        let mut new2 = new.clone();
        let len = new2.len();
        new2.splice(len - 3..len, b"YY\n".iter().copied());
        let hunks = diff_lines(&old, &new2);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].old_start, 1);
        assert!(hunks[1].old_start > 20);
    }

    #[test]
    fn missing_newline_is_a_change() {
        let hunks = diff_lines(b"a\n", b"a");
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].removed(), 1);
        assert_eq!(hunks[0].added(), 1);
    }
}
