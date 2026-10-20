//! Rendering a conflicted file with markers, in Jujutsu's "snapshot + diff"
//! style.
//!
//! # Format
//!
//! A conflict with terms `[a0, r0, a1, r1, ..., aN]` (see [`Merge`]) is
//! rendered per conflicting region as:
//!
//! ```text
//! <<<<<<< Conflict 1 of 2 (2 sides)
//! %%%%%%% Changes from base #1 to side #1
//! -old line
//! +new line
//!  context line
//! +++++++ Contents of side #2
//! other line
//! >>>>>>> Conflict 1 of 2 ends
//! ```
//!
//! For each remove `r_i` there is one `%%%%%%%` section holding the line
//! diff from `r_i` to `a_i` (prefixes ` `, `-`, `+`), followed by one
//! `+++++++` section with the last add `aN` verbatim. Both representations
//! are lossless, so [`parse_materialized`](super::parse_materialized)
//! recovers every term of every conflict region. A content line without a
//! terminator is followed by `\ No newline at end of file` so the next
//! marker still starts a line. Marker lines use the minimum of seven
//! characters, or more when the content itself begins with a run of marker
//! characters; the parser takes the length from the first `<<<<<<<` line.
//!
//! File-level absence (a term that is `None`) is recorded in the header as
//! `absent: term <i>` with one-based interleaved term indices, so a side
//! that deleted the file is distinguishable from one that emptied it.
//!
//! Regions that merge cleanly are written as plain text. Parsing therefore
//! returns a merge whose terms equal the original ones on every conflict
//! region and the auto-merged text elsewhere: an equivalent conflict (the
//! same value under the merge algebra), and one that re-materializes to
//! identical bytes.

use bytes::Bytes;
use rill_types::Merge;

use super::file::{merge_terms, MergeHunk};
use crate::diff::lines::edit_script;
use crate::diff::tokens::LineTokens;

/// Minimum conflict marker length.
pub const MIN_MARKER_LEN: usize = 7;

/// Human-readable names for the sides of a conflict, used in headers only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConflictLabels {
    /// One label per add term, in order; missing labels fall back to
    /// `side #n`.
    pub sides: Vec<String>,
}

impl ConflictLabels {
    fn side(&self, i: usize) -> String {
        match self.sides.get(i) {
            Some(l) => format!("side #{} ({l})", i + 1),
            None => format!("side #{}", i + 1),
        }
    }
}

/// Render a file-level merge with conflict markers.
pub fn materialize(terms: &Merge<Option<Bytes>>) -> Vec<u8> {
    materialize_with(terms, &ConflictLabels::default())
}

/// [`materialize`] with custom side labels.
pub fn materialize_with(terms: &Merge<Option<Bytes>>, labels: &ConflictLabels) -> Vec<u8> {
    let present: Merge<Bytes> = terms.map(|t| t.clone().unwrap_or_default());
    let hunks = merge_terms(&present);
    let conflict_count = hunks
        .iter()
        .filter(|h| matches!(h, MergeHunk::Conflict(_)))
        .count();
    if conflict_count == 0 {
        return hunks
            .iter()
            .filter_map(|h| match h {
                MergeHunk::Resolved(b) => Some(b.as_ref()),
                MergeHunk::Conflict(_) => None,
            })
            .collect::<Vec<_>>()
            .concat();
    }
    let marker_len = marker_len_for(&hunks);
    let absent: Vec<usize> = terms
        .iter()
        .enumerate()
        .filter(|(_, t)| t.is_none())
        .map(|(i, _)| i + 1)
        .collect();

    let mut out = Vec::new();
    let mut index = 0;
    for hunk in &hunks {
        match hunk {
            MergeHunk::Resolved(b) => out.extend_from_slice(b),
            MergeHunk::Conflict(merge) => {
                index += 1;
                write_conflict(
                    &mut out,
                    merge,
                    index,
                    conflict_count,
                    marker_len,
                    &absent,
                    labels,
                );
            }
        }
    }
    out
}

fn write_conflict(
    out: &mut Vec<u8>,
    merge: &Merge<Bytes>,
    index: usize,
    total: usize,
    marker_len: usize,
    absent: &[usize],
    labels: &ConflictLabels,
) {
    let num_sides = merge.num_sides();
    let mut header = format!("Conflict {index} of {total} ({num_sides} sides");
    if !absent.is_empty() {
        let list: Vec<String> = absent.iter().map(|i| format!("term {i}")).collect();
        header.push_str(&format!("; absent: {}", list.join(", ")));
    }
    header.push(')');
    marker(out, b'<', marker_len, &header);

    let adds: Vec<&Bytes> = merge.adds().collect();
    let removes: Vec<&Bytes> = merge.removes().collect();
    for (i, remove) in removes.iter().enumerate() {
        marker(
            out,
            b'%',
            marker_len,
            &format!("Changes from base #{} to {}", i + 1, labels.side(i)),
        );
        write_diff(out, remove, adds[i]);
    }
    let last = num_sides - 1;
    marker(
        out,
        b'+',
        marker_len,
        &format!("Contents of {}", labels.side(last)),
    );
    write_snapshot(out, adds[last]);
    marker(
        out,
        b'>',
        marker_len,
        &format!("Conflict {index} of {total} ends"),
    );
}

fn marker(out: &mut Vec<u8>, ch: u8, len: usize, text: &str) {
    out.extend(std::iter::repeat_n(ch, len));
    out.push(b' ');
    out.extend_from_slice(text.as_bytes());
    out.push(b'\n');
}

/// Write `line` and, if it lacks a terminator, the no-newline marker.
fn write_line(out: &mut Vec<u8>, prefix: Option<u8>, line: &[u8]) {
    if let Some(p) = prefix {
        out.push(p);
    }
    out.extend_from_slice(line);
    if !line.ends_with(b"\n") {
        out.extend_from_slice(NO_NEWLINE);
    }
}

/// Marker line emitted after a content line without a terminator.
pub(crate) const NO_NEWLINE_LINE: &[u8] = b"\\ No newline at end of file\n";

/// Terminator plus [`NO_NEWLINE_LINE`], appended to an unterminated line.
const NO_NEWLINE: &[u8] = b"\n\\ No newline at end of file\n";

fn write_diff(out: &mut Vec<u8>, old: &[u8], new: &[u8]) {
    let old_tokens = LineTokens::new(old);
    let new_tokens = LineTokens::new(new);
    let old_lines = old_tokens.lines();
    let new_lines = new_tokens.lines();
    let mut old_pos = 0;
    for (old_range, new_range) in edit_script(old_lines, new_lines) {
        for line in &old_lines[old_pos..old_range.start] {
            write_line(out, Some(b' '), line);
        }
        for line in &old_lines[old_range.clone()] {
            write_line(out, Some(b'-'), line);
        }
        for line in &new_lines[new_range] {
            write_line(out, Some(b'+'), line);
        }
        old_pos = old_range.end;
    }
    for line in &old_lines[old_pos..] {
        write_line(out, Some(b' '), line);
    }
}

fn write_snapshot(out: &mut Vec<u8>, content: &[u8]) {
    for line in LineTokens::new(content).lines() {
        write_line(out, None, line);
    }
}

/// Pick a marker length longer than any run of marker characters that
/// starts a content line, so content can never be mistaken for a marker.
/// Diff sections prefix content with `+`, which can extend a run by one,
/// hence the margin of two.
fn marker_len_for(hunks: &[MergeHunk]) -> usize {
    let mut longest = 0;
    let mut consider = |bytes: &[u8]| {
        for line in LineTokens::new(bytes).lines() {
            let Some(&first) = line.first() else { continue };
            if matches!(first, b'<' | b'>' | b'%' | b'+' | b'-') {
                let run = line.iter().take_while(|&&b| b == first).count();
                longest = longest.max(run);
            }
        }
    };
    for hunk in hunks {
        match hunk {
            MergeHunk::Resolved(b) => consider(b),
            MergeHunk::Conflict(m) => m.iter().for_each(|t| consider(t)),
        }
    }
    MIN_MARKER_LEN.max(longest + 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(s: &str) -> Option<Bytes> {
        Some(Bytes::copy_from_slice(s.as_bytes()))
    }

    #[test]
    fn renders_three_way_conflict() {
        let m = Merge::three_way(b("a\nb\nc\n"), b("a\nL\nc\n"), b("a\nR\nc\n"));
        let text = String::from_utf8(materialize(&m)).unwrap();
        assert_eq!(
            text,
            "a\n\
             <<<<<<< Conflict 1 of 1 (2 sides)\n\
             %%%%%%% Changes from base #1 to side #1\n\
             -b\n\
             +L\n\
             +++++++ Contents of side #2\n\
             R\n\
             >>>>>>> Conflict 1 of 1 ends\n\
             c\n"
        );
    }

    #[test]
    fn resolved_merge_is_plain() {
        assert_eq!(materialize(&Merge::resolved(b("x\n"))), b"x\n");
        let m = Merge::three_way(b("a\n"), b("a\n"), b("b\n"));
        assert_eq!(materialize(&m), b"b\n");
    }

    #[test]
    fn grows_markers_past_content() {
        let m = Merge::three_way(b("x\n"), b("<<<<<<<<< deep\n"), b("y\n"));
        let text = String::from_utf8(materialize(&m)).unwrap();
        // Content has a run of 9, so the marker is at least 11.
        assert!(text.starts_with("<<<<<<<<<<< Conflict"));
    }
}
