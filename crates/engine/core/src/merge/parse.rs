//! Parsing conflict markers back into merge terms; the inverse of
//! [`materialize`](super::materialize).

use bytes::Bytes;
use rill_types::Merge;

use super::materialize::{MIN_MARKER_LEN, NO_NEWLINE_LINE};
use crate::diff::tokens::split_lines;

/// Parse a materialized file.
///
/// Returns `None` when markers are present but malformed (unterminated
/// conflict, inconsistent side counts, unknown section). A file with no
/// markers parses as a resolved merge.
pub fn parse_materialized(bytes: &[u8]) -> Option<Merge<Option<Bytes>>> {
    let lines: Vec<&[u8]> = split_lines(bytes).collect();
    // Only a `<` run followed by materialize's ` Conflict ` header opens a
    // conflict, so a plain `<<<<<<<` line in a file is content. Content runs
    // are always shorter than the marker, so the longest header run is the
    // marker length.
    let Some(marker_len) = lines.iter().filter_map(|l| conflict_header(l)).max() else {
        return Some(Merge::resolved(Some(Bytes::copy_from_slice(bytes))));
    };

    // Per-term accumulated content; the term count is fixed by the first
    // conflict.
    let mut terms: Option<Vec<Vec<u8>>> = None;
    let mut absent: Vec<usize> = Vec::new();
    let mut resolved: Vec<u8> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if conflict_header(line) != Some(marker_len) {
            resolved.extend_from_slice(line);
            i += 1;
            continue;
        }
        absent = parse_absent(&line[marker_len..]);
        i += 1;
        let (conflict, next) = parse_conflict(&lines, i, marker_len)?;
        i = next;
        let n = conflict.len();
        let terms = terms.get_or_insert_with(|| vec![Vec::new(); n]);
        if terms.len() != n {
            return None;
        }
        for t in terms.iter_mut() {
            t.extend_from_slice(&resolved);
        }
        resolved.clear();
        for (t, part) in terms.iter_mut().zip(conflict) {
            t.extend_from_slice(&part);
        }
    }
    let Some(mut terms) = terms else {
        return Some(Merge::resolved(Some(Bytes::copy_from_slice(bytes))));
    };
    for t in terms.iter_mut() {
        t.extend_from_slice(&resolved);
    }
    let values: Vec<Option<Bytes>> = terms
        .into_iter()
        .enumerate()
        .map(|(idx, t)| {
            if absent.contains(&(idx + 1)) {
                None
            } else {
                Some(Bytes::from(t))
            }
        })
        .collect();
    Merge::from_vec(values).ok()
}

/// A piece of a file with conflict markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Region {
    /// Text outside any conflict.
    Text(Bytes),
    /// One conflict: its marker text as written, and its terms (sides and
    /// bases interleaved, as in [`Merge`]).
    Conflict { raw: Bytes, terms: Merge<Bytes> },
}

/// The regions of a file with conflict markers, in order, so each
/// conflict can be resolved on its own; `None` for malformed markers.
pub fn parse_regions(bytes: &[u8]) -> Option<Vec<Region>> {
    let lines: Vec<&[u8]> = split_lines(bytes).collect();
    let Some(marker_len) = lines.iter().filter_map(|l| conflict_header(l)).max() else {
        return Some(vec![Region::Text(Bytes::copy_from_slice(bytes))]);
    };
    let mut out = Vec::new();
    let mut text = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if conflict_header(lines[i]) != Some(marker_len) {
            text.extend_from_slice(lines[i]);
            i += 1;
            continue;
        }
        if !text.is_empty() {
            out.push(Region::Text(Bytes::from(std::mem::take(&mut text))));
        }
        let start = i;
        let (parts, next) = parse_conflict(&lines, i + 1, marker_len)?;
        let raw: Vec<u8> = lines[start..next].concat();
        let terms = Merge::from_vec(parts.into_iter().map(Bytes::from).collect()).ok()?;
        out.push(Region::Conflict {
            raw: Bytes::from(raw),
            terms,
        });
        i = next;
    }
    if !text.is_empty() {
        out.push(Region::Text(Bytes::from(text)));
    }
    Some(out)
}

/// Parse the sections of one conflict starting after its header; returns
/// the interleaved terms and the index after the end marker.
fn parse_conflict(
    lines: &[&[u8]],
    mut i: usize,
    marker_len: usize,
) -> Option<(Vec<Vec<u8>>, usize)> {
    let mut adds: Vec<Vec<u8>> = Vec::new();
    let mut removes: Vec<Vec<u8>> = Vec::new();
    loop {
        let header = *lines.get(i)?;
        i += 1;
        if marker_run(header, b'>') == Some(marker_len) {
            break;
        }
        let kind = if marker_run(header, b'%') == Some(marker_len) {
            b'%'
        } else if marker_run(header, b'+') == Some(marker_len) {
            b'+'
        } else {
            return None;
        };
        let mut add = Vec::new();
        let mut remove = Vec::new();
        // Which buffers received the previous line, so a no-newline marker
        // strips the terminator from exactly those.
        let mut last = (false, false);
        while let Some(&line) = lines.get(i) {
            if is_any_marker(line, marker_len) {
                break;
            }
            i += 1;
            if line.starts_with(NO_NEWLINE_LINE) {
                if last.0 {
                    strip_newline(&mut add);
                }
                if last.1 {
                    strip_newline(&mut remove);
                }
                continue;
            }
            match kind {
                b'+' => {
                    add.extend_from_slice(line);
                    last = (true, false);
                }
                _ => {
                    let (prefix, rest) = line.split_first()?;
                    last = match prefix {
                        b' ' => {
                            add.extend_from_slice(rest);
                            remove.extend_from_slice(rest);
                            (true, true)
                        }
                        b'+' => {
                            add.extend_from_slice(rest);
                            (true, false)
                        }
                        b'-' => {
                            remove.extend_from_slice(rest);
                            (false, true)
                        }
                        _ => return None,
                    };
                }
            }
        }
        adds.push(add);
        if kind == b'%' {
            removes.push(remove);
        }
    }
    if adds.len() != removes.len() + 1 {
        return None;
    }
    let mut terms = Vec::with_capacity(adds.len() + removes.len());
    let mut removes = removes.into_iter();
    for (idx, add) in adds.into_iter().enumerate() {
        if idx > 0 {
            terms.push(removes.next()?);
        }
        terms.push(add);
    }
    Some((terms, i))
}

fn strip_newline(buf: &mut Vec<u8>) {
    if buf.ends_with(b"\n") {
        buf.pop();
    }
}

fn marker_run(line: &[u8], ch: u8) -> Option<usize> {
    let run = line.iter().take_while(|&&b| b == ch).count();
    if run >= MIN_MARKER_LEN && matches!(line.get(run), Some(b' ') | Some(b'\n') | None) {
        Some(run)
    } else {
        None
    }
}

/// The marker length of a conflict's opening line, `<<<<<<< Conflict ...`.
fn conflict_header(line: &[u8]) -> Option<usize> {
    let run = marker_run(line, b'<')?;
    line[run..].starts_with(b" Conflict ").then_some(run)
}

fn is_any_marker(line: &[u8], marker_len: usize) -> bool {
    b"<>%+"
        .iter()
        .any(|&c| marker_run(line, c) == Some(marker_len))
}

fn parse_absent(header: &[u8]) -> Vec<usize> {
    let text = String::from_utf8_lossy(header);
    let Some((_, rest)) = text.split_once("absent:") else {
        return Vec::new();
    };
    rest.trim_end_matches(|c: char| c == ')' || c.is_whitespace())
        .split(',')
        .filter_map(|part| part.trim().strip_prefix("term ")?.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::materialize;

    fn b(s: &str) -> Option<Bytes> {
        Some(Bytes::copy_from_slice(s.as_bytes()))
    }

    fn roundtrip(m: &Merge<Option<Bytes>>) {
        let text = materialize(m);
        let parsed = parse_materialized(&text).expect("parses");
        assert_eq!(&parsed, m, "{}", String::from_utf8_lossy(&text));
        assert_eq!(materialize(&parsed), text);
    }

    #[test]
    fn roundtrips_simple_conflict() {
        roundtrip(&Merge::three_way(
            b("a\nb\nc\n"),
            b("a\nL\nc\n"),
            b("a\nR\nc\n"),
        ));
    }

    #[test]
    fn roundtrips_whole_file_conflict() {
        roundtrip(&Merge::three_way(b("x\n"), b("y\n"), b("z\n")));
        roundtrip(&Merge::three_way(b(""), b("y\n"), b("z\n")));
    }

    #[test]
    fn roundtrips_absent_terms() {
        roundtrip(&Merge::three_way(b("x\n"), None, b("z\n")));
        roundtrip(&Merge::three_way(None, b("y\n"), b("z\n")));
    }

    #[test]
    fn roundtrips_missing_final_newline() {
        roundtrip(&Merge::three_way(b("a\nb"), b("a\nL"), b("a\nR\n")));
        roundtrip(&Merge::three_way(b("b"), b("L"), b("R")));
    }

    #[test]
    fn roundtrips_many_sides_and_marker_like_content() {
        let m = Merge::from_vec(vec![
            b("<<<<<<< one\n"),
            b("base1\n"),
            b("two\n"),
            b("base2\n"),
            b("+++++++++ three\n"),
        ])
        .unwrap();
        roundtrip(&m);
    }

    #[test]
    fn equivalent_after_partial_resolution() {
        // Left changes line 1, both conflict on line 3: the parsed merge
        // has the same value and re-materializes identically.
        let m = Merge::three_way(b("a\nb\nc\n"), b("A\nb\nL\n"), b("a\nb\nR\n"));
        let text = materialize(&m);
        let parsed = parse_materialized(&text).unwrap();
        assert_eq!(materialize(&parsed), text);
        assert_eq!(
            parsed,
            Merge::three_way(b("A\nb\nc\n"), b("A\nb\nL\n"), b("A\nb\nR\n"))
        );
    }

    #[test]
    fn plain_file_and_malformed() {
        assert_eq!(
            parse_materialized(b"hello\n"),
            Some(Merge::resolved(b("hello\n")))
        );
        assert!(
            parse_materialized(b"<<<<<<< Conflict 1 of 1 (2 sides)\n%%%%%%% x\n-a\n").is_none()
        );
    }
}

#[cfg(test)]
mod region_tests {
    use super::*;
    use crate::merge::materialize;

    #[test]
    fn regions_split_text_from_each_conflict() {
        let terms = Merge::three_way(
            Some(Bytes::from_static(b"a\nb\nc\nd\ne\n")),
            Some(Bytes::from_static(b"a\nB1\nc\nd\nE1\n")),
            Some(Bytes::from_static(b"a\nB2\nc\nd\nE2\n")),
        );
        let text = materialize(&terms);
        let regions = parse_regions(&text).unwrap();
        let conflicts: Vec<&Merge<Bytes>> = regions
            .iter()
            .filter_map(|r| match r {
                Region::Conflict { terms, .. } => Some(terms),
                Region::Text(_) => None,
            })
            .collect();
        assert_eq!(conflicts.len(), 2);
        let sides: Vec<&Bytes> = conflicts[0].adds().collect();
        assert_eq!(&sides[0][..], b"B1\n");
        assert_eq!(&sides[1][..], b"B2\n");
        assert_eq!(&conflicts[0].removes().next().unwrap()[..], b"b\n");
        let rebuilt: Vec<u8> = regions
            .iter()
            .flat_map(|r| match r {
                Region::Text(t) => t.to_vec(),
                Region::Conflict { raw, .. } => raw.to_vec(),
            })
            .collect();
        assert_eq!(rebuilt, text, "raw regions rebuild the file");
    }
}
