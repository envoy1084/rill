//! Applying unified diffs.
//!
//! Supports the subset [`to_unified`](super::to_unified) emits: `---`/`+++`
//! headers, `@@` hunks, ` `/`-`/`+` lines and `\ No newline at end of file`.
//! A hunk must match its context exactly at the stated line or within a
//! small window around it; anything else is rejected rather than guessed
//! at, because a misapplied patch is worse than a failed one.

use super::tokens::{line_content, split_lines};
use crate::error::{Error, Result};

/// How far from the stated line a hunk may be shifted to find its context.
const FUZZ_WINDOW: usize = 50;

#[derive(Debug)]
enum Line {
    Context(Vec<u8>),
    Remove(Vec<u8>),
    Add(Vec<u8>),
}

#[derive(Debug)]
struct Hunk {
    old_start: usize,
    lines: Vec<Line>,
}

/// Apply `patch` to `old`, returning the new content.
pub fn apply_unified(old: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
    let hunks = parse(patch)?;
    let old_lines: Vec<&[u8]> = split_lines(old).collect();
    let mut out: Vec<u8> = Vec::with_capacity(old.len());
    let mut cursor = 0usize;
    for hunk in &hunks {
        let expected: Vec<&[u8]> = hunk
            .lines
            .iter()
            .filter_map(|l| match l {
                Line::Context(t) | Line::Remove(t) => Some(t.as_slice()),
                Line::Add(_) => None,
            })
            .collect();
        let stated = hunk.old_start.saturating_sub(1).max(cursor);
        let at = locate(&old_lines, &expected, stated, cursor).ok_or_else(|| {
            invalid(format!(
                "hunk at line {} does not match the base content",
                hunk.old_start
            ))
        })?;
        old_lines[cursor..at]
            .iter()
            .for_each(|l| out.extend_from_slice(l));
        let mut pos = at;
        for line in &hunk.lines {
            match line {
                Line::Context(t) => {
                    out.extend_from_slice(t);
                    pos += 1;
                }
                Line::Remove(_) => pos += 1,
                Line::Add(t) => out.extend_from_slice(t),
            }
        }
        cursor = pos;
    }
    old_lines[cursor..]
        .iter()
        .for_each(|l| out.extend_from_slice(l));
    Ok(out)
}

fn locate(old: &[&[u8]], expected: &[&[u8]], stated: usize, min: usize) -> Option<usize> {
    let matches_at = |at: usize| {
        at + expected.len() <= old.len()
            && expected
                .iter()
                .zip(&old[at..])
                .all(|(e, o)| line_content(e) == line_content(o))
    };
    if expected.is_empty() {
        return Some(stated.min(old.len()));
    }
    if matches_at(stated) {
        return Some(stated);
    }
    (1..=FUZZ_WINDOW).find_map(|delta| {
        if matches_at(stated + delta) {
            Some(stated + delta)
        } else if stated >= min + delta && matches_at(stated - delta) {
            Some(stated - delta)
        } else {
            None
        }
    })
}

fn parse(patch: &[u8]) -> Result<Vec<Hunk>> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut lines = split_lines(patch).peekable();
    while let Some(raw) = lines.next() {
        let line = raw.strip_suffix(b"\n").unwrap_or(raw);
        if line.starts_with(b"--- ") || line.starts_with(b"+++ ") || line.starts_with(b"diff ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix(b"@@ ") {
            hunks.push(Hunk {
                old_start: parse_header(rest)?,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            return Err(invalid(format!(
                "line outside any hunk: {:?}",
                String::from_utf8_lossy(line)
            )));
        };
        if raw.starts_with(b"\\ No newline") {
            if let Some(Line::Context(t) | Line::Remove(t) | Line::Add(t)) = hunk.lines.last_mut() {
                if t.last() == Some(&b'\n') {
                    t.pop();
                }
            }
            continue;
        }
        let Some((&tag, text)) = raw.split_first() else {
            continue;
        };
        let mut body = text.to_vec();
        if !body.ends_with(b"\n") && lines.peek().is_some() {
            body.push(b'\n');
        }
        hunk.lines.push(match tag {
            b' ' => Line::Context(body),
            b'-' => Line::Remove(body),
            b'+' => Line::Add(body),
            // Some tools drop the space on empty context lines.
            b'\n' => Line::Context(b"\n".to_vec()),
            other => {
                return Err(invalid(format!(
                    "unexpected line prefix {:?}",
                    other as char
                )))
            }
        });
    }
    Ok(hunks)
}

/// Parse `-a[,b] +c[,d] @@...` and return the old start line.
fn parse_header(rest: &[u8]) -> Result<usize> {
    let text = String::from_utf8_lossy(rest);
    let bad = || invalid(format!("malformed hunk header @@ {text}"));
    let old = text.split_whitespace().next().ok_or_else(bad)?;
    let start = old.strip_prefix('-').ok_or_else(bad)?;
    start
        .split(',')
        .next()
        .ok_or_else(bad)?
        .parse()
        .map_err(|_| bad())
}

fn invalid(reason: String) -> Error {
    Error::InvalidPatch(reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{diff_lines, diff_lines_with_context, to_unified};

    fn roundtrip(old: &[u8], new: &[u8]) {
        let patch = to_unified("f", &diff_lines(old, new));
        let applied = apply_unified(old, &patch).unwrap();
        assert_eq!(applied, new, "patch:\n{}", String::from_utf8_lossy(&patch));
    }

    #[test]
    fn roundtrips_rendered_diffs() {
        roundtrip(b"a\nb\nc\n", b"a\nB\nc\n");
        roundtrip(b"", b"hello\nworld\n");
        roundtrip(b"x\ny\n", b"");
        roundtrip(
            b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n",
            b"0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n",
        );
        roundtrip(b"a\nb", b"a\nb\nc");
        roundtrip(b"a\nb\n", b"a\nb");
        roundtrip(b"a\r\nb\r\n", b"a\r\nB\r\n");
    }

    #[test]
    fn roundtrips_non_utf8_content_byte_for_byte() {
        let old = b"caf\xe9\nna\xefve\n\xff\xfe\n";
        let new = b"caf\xe9\nna\xefvet\xe9\n\xff\xfe\n";
        roundtrip(old, new);
    }

    #[test]
    fn finds_shifted_context_but_rejects_mismatch() {
        let patch = to_unified("f", &diff_lines_with_context(b"a\nb\nc\n", b"a\nB\nc\n", 1));
        assert_eq!(
            apply_unified(b"zero\na\nb\nc\n", &patch).unwrap(),
            b"zero\na\nB\nc\n"
        );
        assert!(apply_unified(b"x\ny\n", &patch).is_err());
        assert!(apply_unified(b"", b"garbage\n").is_err());
    }
}
