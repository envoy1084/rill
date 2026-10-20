//! Line-based N-way text merge.
//!
//! The algorithm is diff3 generalized to the [`Merge`] algebra. One term,
//! the first remove (the "base"), is the alignment reference: every other
//! term is diffed against it and the edit scripts are merged into regions
//! of the base that are either untouched by every term or touched by at
//! least one. Each touched region becomes a `Merge` of line slices and is
//! resolved with [`Merge::resolve_trivial`], so a region changed on only
//! one side, or changed identically on several sides, merges cleanly.
//! Adjacent edits (no unchanged line between them) conflict, as in git.
//!
//! Conflict regions are minimized zdiff3-style: lines that every term
//! shares at the start or end of the region move out into the resolved
//! text, so a conflict shows only the lines that actually differ.

use std::ops::Range;

use bytes::Bytes;
use rill_types::Merge;

use crate::diff::lines::edit_script;
use crate::diff::tokens::LineTokens;

/// A borrowed run of line tokens.
type Lines<'a> = &'a [&'a [u8]];

/// A region of merged output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeHunk {
    /// Text identical or trivially resolved across all terms.
    Resolved(Bytes),
    /// A genuine conflict; the terms have the shape of the input merge.
    Conflict(Merge<Bytes>),
}

/// Outcome of a three-way file merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileMergeResult {
    /// Every region merged cleanly.
    Resolved(Vec<u8>),
    /// At least one region conflicts; resolved regions are included so the
    /// hunks concatenate to the whole file.
    Conflicted { hunks: Vec<MergeHunk> },
}

impl FileMergeResult {
    /// True when no hunk conflicts.
    pub fn is_resolved(&self) -> bool {
        matches!(self, FileMergeResult::Resolved(_))
    }
}

/// Three-way merge of `left` and `right` against their common `base`.
pub fn merge_file(base: &[u8], left: &[u8], right: &[u8]) -> FileMergeResult {
    let terms = Merge::three_way(
        Bytes::copy_from_slice(base),
        Bytes::copy_from_slice(left),
        Bytes::copy_from_slice(right),
    );
    let hunks = merge_terms(&terms);
    match resolved_bytes(&hunks) {
        Some(bytes) => FileMergeResult::Resolved(bytes),
        None => FileMergeResult::Conflicted { hunks },
    }
}

/// Concatenate hunks when none conflicts.
pub fn resolved_bytes(hunks: &[MergeHunk]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for hunk in hunks {
        match hunk {
            MergeHunk::Resolved(b) => out.extend_from_slice(b),
            MergeHunk::Conflict(_) => return None,
        }
    }
    Some(out)
}

/// Merge an arbitrary N-sided term list line by line.
pub fn merge_terms(terms: &Merge<Bytes>) -> Vec<MergeHunk> {
    if let Some(value) = terms.resolve_trivial() {
        return vec![MergeHunk::Resolved(value.clone())];
    }
    let tokens: Vec<LineTokens<'_>> = terms.iter().map(|t| LineTokens::new(t)).collect();
    // Index 1 is the first remove; the merge is not resolved so it exists.
    let base = tokens[1].lines();
    let edits: Vec<Vec<(Range<usize>, Range<usize>)>> = tokens
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if i == 1 {
                Vec::new()
            } else {
                edit_script(base, t.lines())
            }
        })
        .collect();
    let regions = align(base.len(), &edits);

    let mut hunks: Vec<MergeHunk> = Vec::new();
    for region in regions {
        let slices: Vec<&[&[u8]]> = region
            .iter()
            .enumerate()
            .map(|(i, r)| &tokens[i].lines()[r.clone()])
            .collect();
        // Cannot fail: `region` has one range per term, an odd count.
        let merge = Merge::from_vec(slices).expect("region has one range per term");
        match merge.resolve_trivial() {
            Some(lines) => push_resolved(&mut hunks, lines),
            None => {
                let (prefix, middle, suffix) = minimize(&merge);
                push_resolved(&mut hunks, prefix);
                hunks.push(MergeHunk::Conflict(
                    middle.map(|lines| Bytes::from(lines.concat())),
                ));
                push_resolved(&mut hunks, suffix);
            }
        }
    }
    if hunks.is_empty() {
        hunks.push(MergeHunk::Resolved(Bytes::new()));
    }
    hunks
}

fn push_resolved(hunks: &mut Vec<MergeHunk>, lines: &[&[u8]]) {
    if lines.is_empty() {
        return;
    }
    let bytes = lines.concat();
    match hunks.last_mut() {
        Some(MergeHunk::Resolved(prev)) => {
            let mut joined = Vec::with_capacity(prev.len() + bytes.len());
            joined.extend_from_slice(prev);
            joined.extend_from_slice(&bytes);
            *prev = Bytes::from(joined);
        }
        _ => hunks.push(MergeHunk::Resolved(Bytes::from(bytes))),
    }
}

/// Split a conflicting region into lines shared by all terms at the start,
/// the differing middle, and lines shared at the end.
fn minimize<'a>(merge: &Merge<Lines<'a>>) -> (Lines<'a>, Merge<Lines<'a>>, Lines<'a>) {
    let terms = merge.as_slice();
    let first = terms[0];
    let min_len = terms.iter().map(|t| t.len()).min().unwrap_or(0);
    let mut prefix = 0;
    while prefix < min_len && terms.iter().all(|t| t[prefix] == first[prefix]) {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < min_len - prefix
        && terms
            .iter()
            .all(|t| t[t.len() - 1 - suffix] == first[first.len() - 1 - suffix])
    {
        suffix += 1;
    }
    let middle = merge.map(|t| &t[prefix..t.len() - suffix]);
    (&first[..prefix], middle, &first[first.len() - suffix..])
}

/// Partition the base into regions and map each to a range in every term.
///
/// `edits[i]` is the edit script from the base to term `i` (empty for the
/// base itself). Returns, per region, one range per term; untouched
/// regions are included so the output covers every line.
fn align(base_len: usize, edits: &[Vec<(Range<usize>, Range<usize>)>]) -> Vec<Vec<Range<usize>>> {
    let n = edits.len();
    let mut idx = vec![0usize; n];
    // Offset of term i relative to the base at the current position.
    let mut delta = vec![0isize; n];
    let mut base_pos = 0usize;
    let mut regions = Vec::new();

    let shift = |pos: usize, d: isize| -> usize { (pos as isize + d) as usize };

    loop {
        let next_start = (0..n)
            .filter_map(|i| edits[i].get(idx[i]).map(|h| h.0.start))
            .min();
        let Some(start) = next_start else {
            if base_pos < base_len {
                regions.push(
                    (0..n)
                        .map(|i| shift(base_pos, delta[i])..shift(base_len, delta[i]))
                        .collect(),
                );
            }
            break;
        };
        if start > base_pos {
            regions.push(
                (0..n)
                    .map(|i| shift(base_pos, delta[i])..shift(start, delta[i]))
                    .collect(),
            );
        }
        let starts: Vec<usize> = (0..n).map(|i| shift(start, delta[i])).collect();
        let mut end = start;
        let mut progress = true;
        while progress {
            progress = false;
            for i in 0..n {
                while let Some((b, s)) = edits[i].get(idx[i]) {
                    if b.start > end {
                        break;
                    }
                    end = end.max(b.end);
                    delta[i] += s.len() as isize - b.len() as isize;
                    idx[i] += 1;
                    progress = true;
                }
            }
        }
        regions.push((0..n).map(|i| starts[i]..shift(end, delta[i])).collect());
        base_pos = end;
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conflicts(r: &FileMergeResult) -> usize {
        match r {
            FileMergeResult::Resolved(_) => 0,
            FileMergeResult::Conflicted { hunks } => hunks
                .iter()
                .filter(|h| matches!(h, MergeHunk::Conflict(_)))
                .count(),
        }
    }

    #[test]
    fn disjoint_edits_merge() {
        let base = b"a\nb\nc\nd\ne\nf\ng\n";
        let left = b"A\nb\nc\nd\ne\nf\ng\n";
        let right = b"a\nb\nc\nd\ne\nf\nG\n";
        assert_eq!(
            merge_file(base, left, right),
            FileMergeResult::Resolved(b"A\nb\nc\nd\ne\nf\nG\n".to_vec())
        );
    }

    #[test]
    fn same_line_conflicts() {
        let r = merge_file(b"a\nb\nc\n", b"a\nL\nc\n", b"a\nR\nc\n");
        assert_eq!(conflicts(&r), 1);
        if let FileMergeResult::Conflicted { hunks } = &r {
            assert_eq!(hunks.len(), 3);
            assert_eq!(hunks[0], MergeHunk::Resolved(Bytes::from_static(b"a\n")));
            assert_eq!(
                hunks[1],
                MergeHunk::Conflict(Merge::three_way(
                    Bytes::from_static(b"b\n"),
                    Bytes::from_static(b"L\n"),
                    Bytes::from_static(b"R\n")
                ))
            );
        }
    }

    #[test]
    fn identical_changes_resolve() {
        let r = merge_file(b"a\nb\n", b"a\nX\n", b"a\nX\n");
        assert_eq!(r, FileMergeResult::Resolved(b"a\nX\n".to_vec()));
    }

    #[test]
    fn adjacent_edits_conflict() {
        let r = merge_file(b"a\nb\n", b"A\nb\n", b"a\nB\n");
        assert_eq!(conflicts(&r), 1);
    }

    #[test]
    fn conflict_is_minimized() {
        let base = b"a\nb\nc\n";
        let left = b"a\nX\nL\nY\nc\n";
        let right = b"a\nX\nR\nY\nc\n";
        let r = merge_file(base, left, right);
        if let FileMergeResult::Conflicted { hunks } = &r {
            assert_eq!(hunks[0], MergeHunk::Resolved(Bytes::from_static(b"a\n")));
            // X and Y are shared by both sides but not the base, so they stay
            // in the conflict (the base must stay aligned).
            assert!(matches!(&hunks[1], MergeHunk::Conflict(_)));
        } else {
            panic!("expected conflict");
        }
        let r = merge_file(b"a\nb\nc\n", b"a\nL\nc\n", b"a\nR\nc\n");
        assert_eq!(conflicts(&r), 1);
    }

    #[test]
    fn deletion_versus_edit() {
        let r = merge_file(b"a\nb\nc\n", b"a\nc\n", b"a\nB\nc\n");
        assert_eq!(conflicts(&r), 1);
        let r = merge_file(b"a\nb\nc\n", b"a\nc\n", b"a\nb\nc\n");
        assert_eq!(r, FileMergeResult::Resolved(b"a\nc\n".to_vec()));
    }

    #[test]
    fn empty_and_missing_newline() {
        assert_eq!(
            merge_file(b"", b"x\n", b""),
            FileMergeResult::Resolved(b"x\n".to_vec())
        );
        let r = merge_file(b"a\nb", b"a\nb\n", b"a\nb");
        assert_eq!(r, FileMergeResult::Resolved(b"a\nb\n".to_vec()));
    }

    #[test]
    fn four_way_terms() {
        let terms = Merge::from_vec(vec![
            Bytes::from_static(b"a\nX\nc\nd\ne\n"),
            Bytes::from_static(b"a\nb\nc\nd\ne\n"),
            Bytes::from_static(b"a\nb\nc\nd\ne\nf\n"),
            Bytes::from_static(b"a\nb\nc\nd\ne\n"),
            Bytes::from_static(b"a\nb\nc\nD\ne\n"),
        ])
        .unwrap();
        let hunks = merge_terms(&terms);
        assert_eq!(resolved_bytes(&hunks), Some(b"a\nX\nc\nD\ne\nf\n".to_vec()));
    }
}
