//! Unified diff rendering.

use super::hunk::{Hunk, LineTag};

/// Render hunks as a unified diff for `path`.
///
/// Output is bytes, not text: line content is copied verbatim so that
/// [`apply_unified`](super::apply_unified) reproduces non-UTF-8 files exactly.
/// The amount of context is whatever the hunks were built with.
pub fn to_unified(path: &str, hunks: &[Hunk]) -> Vec<u8> {
    let mut out = Vec::new();
    if hunks.is_empty() {
        return out;
    }
    let old_label = if hunks.len() == 1 && hunks[0].old_len == 0 {
        "/dev/null".to_string()
    } else {
        format!("a/{path}")
    };
    let new_label = if hunks.len() == 1 && hunks[0].new_len == 0 {
        "/dev/null".to_string()
    } else {
        format!("b/{path}")
    };
    out.extend_from_slice(format!("--- {old_label}\n+++ {new_label}\n").as_bytes());
    for hunk in hunks {
        let header = format!(
            "@@ -{} +{} @@\n",
            range_label(hunk.old_start, hunk.old_len),
            range_label(hunk.new_start, hunk.new_len)
        );
        out.extend_from_slice(header.as_bytes());
        for line in &hunk.lines {
            out.push(match line.tag {
                LineTag::Context => b' ',
                LineTag::Removed => b'-',
                LineTag::Added => b'+',
            });
            out.extend_from_slice(&line.text);
            if !line.text.ends_with(b"\n") {
                out.extend_from_slice(b"\n\\ No newline at end of file\n");
            }
        }
    }
    out
}

fn range_label(start: u32, len: u32) -> String {
    match len {
        1 => start.to_string(),
        0 => format!("{},0", start.saturating_sub(1)),
        _ => format!("{start},{len}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{diff_lines, diff_lines_with_context};

    #[test]
    fn matches_diff_u_output() {
        let text = to_unified("f.txt", &diff_lines(b"a\nb\nc\n", b"a\nB\nc\n"));
        assert_eq!(
            text,
            b"--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n"
        );
        let added = to_unified("f", &diff_lines(b"", b"x\n"));
        assert!(added.starts_with(b"--- /dev/null\n+++ b/f\n@@ -0,0 +1 @@\n+x\n"));
    }

    #[test]
    fn context_follows_the_hunks() {
        let old = b"1\n2\n3\n4\n5\n6\n7\n";
        let new = b"1\n2\n3\nX\n5\n6\n7\n";
        let tight = to_unified("f", &diff_lines_with_context(old, new, 0));
        assert_eq!(tight, b"--- a/f\n+++ b/f\n@@ -4 +4 @@\n-4\n+X\n");
        let wide = to_unified("f", &diff_lines_with_context(old, new, 1));
        assert_eq!(wide, b"--- a/f\n+++ b/f\n@@ -3,3 +3,3 @@\n 3\n-4\n+X\n 5\n");
    }
}
