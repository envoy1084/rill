//! A small unified-diff applier for `ApplyEdits` patches.
//!
//! Supports the subset `rill-core` emits (`---`/`+++` headers, `@@` hunks,
//! ` `/`-`/`+` lines and `\ No newline at end of file`). Hunks must match
//! their context exactly at the stated line, or within a small window
//! around it; anything else is rejected rather than guessed at, since a
//! misapplied patch is far worse than a failed one.

use crate::error::{Error, Result};

/// How far from the stated line a hunk may be shifted to find its context.
const FUZZ_WINDOW: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    Context(Vec<u8>),
    Remove(Vec<u8>),
    Add(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Hunk {
    old_start: usize,
    lines: Vec<Line>,
}

/// Apply `patch` (unified diff text) to `old`, returning the new content.
pub fn apply_unified(old: &[u8], patch: &str) -> Result<Vec<u8>> {
    let hunks = parse(patch)?;
    let old_lines: Vec<Vec<u8>> = rill_core::diff::split_lines(old)
        .map(<[u8]>::to_vec)
        .collect();
    let mut out: Vec<Vec<u8>> = Vec::with_capacity(old_lines.len());
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
            Error::invalid(format!(
                "patch hunk at line {} does not match the base content",
                hunk.old_start
            ))
        })?;
        out.extend(old_lines[cursor..at].iter().cloned());
        let mut pos = at;
        for line in &hunk.lines {
            match line {
                Line::Context(t) => {
                    out.push(t.clone());
                    pos += 1;
                }
                Line::Remove(_) => pos += 1,
                Line::Add(t) => out.push(t.clone()),
            }
        }
        cursor = pos;
    }
    out.extend(old_lines[cursor..].iter().cloned());
    Ok(out.concat())
}

fn locate(old: &[Vec<u8>], expected: &[&[u8]], stated: usize, min: usize) -> Option<usize> {
    let matches_at = |at: usize| {
        at + expected.len() <= old.len()
            && expected
                .iter()
                .zip(&old[at..])
                .all(|(e, o)| strip_eol(e) == strip_eol(o))
    };
    if expected.is_empty() {
        return Some(stated.min(old.len()));
    }
    if matches_at(stated) {
        return Some(stated);
    }
    for delta in 1..=FUZZ_WINDOW {
        if stated + delta + expected.len() <= old.len() && matches_at(stated + delta) {
            return Some(stated + delta);
        }
        if stated >= delta && stated - delta >= min && matches_at(stated - delta) {
            return Some(stated - delta);
        }
    }
    None
}

fn strip_eol(line: &[u8]) -> &[u8] {
    rill_core::diff::line_content(line)
}

fn parse(patch: &str) -> Result<Vec<Hunk>> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut lines = patch.split_inclusive('\n').peekable();
    while let Some(raw) = lines.next() {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        if line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("diff ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("@@ ") {
            let old_start = parse_header(rest)?;
            hunks.push(Hunk {
                old_start,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            if line.trim().is_empty() {
                continue;
            }
            return Err(Error::invalid(format!(
                "patch line outside any hunk: {line:?}"
            )));
        };
        if let Some(rest) = raw.strip_prefix("\\ No newline") {
            let _ = rest;
            if let Some(last) = hunk.lines.last_mut() {
                let text = match last {
                    Line::Context(t) | Line::Remove(t) | Line::Add(t) => t,
                };
                if text.last() == Some(&b'\n') {
                    text.pop();
                }
            }
            continue;
        }
        let (tag, text) = raw.split_at(1.min(raw.len()));
        let mut body = text.as_bytes().to_vec();
        if raw.is_empty() {
            continue;
        }
        if !body.ends_with(b"\n") && lines.peek().is_some() {
            body.push(b'\n');
        }
        hunk.lines.push(match tag {
            " " => Line::Context(body),
            "-" => Line::Remove(body),
            "+" => Line::Add(body),
            // A bare newline inside a hunk is an empty context line.
            "\n" => Line::Context(b"\n".to_vec()),
            other => {
                return Err(Error::invalid(format!(
                    "unexpected patch line prefix {other:?}"
                )))
            }
        });
    }
    Ok(hunks)
}

/// Parse `-a[,b] +c[,d] @@...` and return the old start line.
fn parse_header(rest: &str) -> Result<usize> {
    let bad = || Error::invalid(format!("malformed hunk header @@ {rest}"));
    let old = rest.split_whitespace().next().ok_or_else(bad)?;
    let old = old.strip_prefix('-').ok_or_else(bad)?;
    let start = old.split(',').next().ok_or_else(bad)?;
    start.parse::<usize>().map_err(|_| bad())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_core::{diff_lines, to_unified};

    fn roundtrip(old: &str, new: &str) {
        let patch = to_unified("f", &diff_lines(old.as_bytes(), new.as_bytes()), 3);
        let applied = apply_unified(old.as_bytes(), &patch).unwrap();
        assert_eq!(String::from_utf8(applied).unwrap(), new, "patch:\n{patch}");
    }

    #[test]
    fn applies_core_diffs() {
        roundtrip("a\nb\nc\n", "a\nB\nc\n");
        roundtrip("", "hello\nworld\n");
        roundtrip("x\ny\n", "");
        roundtrip(
            "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n",
            "0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n",
        );
        roundtrip("a\nb", "a\nb\nc");
        roundtrip("a\nb\n", "a\nb");
        roundtrip(
            "fn main() {\n    println!(\"hi\");\n}\n",
            "fn main() {\n    println!(\"hello\");\n    println!(\"world\");\n}\n",
        );
    }

    #[test]
    fn rejects_mismatch() {
        let patch = to_unified("f", &diff_lines(b"a\nb\n", b"a\nB\n"), 3);
        assert!(apply_unified(b"x\ny\n", &patch).is_err());
        assert!(apply_unified(b"", "garbage\n").is_err());
    }

    #[test]
    fn tolerates_shifted_context() {
        let patch = to_unified("f", &diff_lines(b"a\nb\nc\n", b"a\nB\nc\n"), 1);
        let shifted = b"zero\na\nb\nc\n";
        assert_eq!(apply_unified(shifted, &patch).unwrap(), b"zero\na\nB\nc\n");
    }
}
