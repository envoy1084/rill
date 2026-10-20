//! Unified diff rendering.

use super::hunk::{Hunk, LineTag};

/// Render hunks as a unified diff for `path`.
///
/// `context` is informational only: the hunks already carry their context
/// lines; it is used to decide how to label completely new or removed
/// files.
pub fn to_unified(path: &str, hunks: &[Hunk], context: usize) -> String {
    let _ = context;
    let mut out = String::new();
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
    out.push_str(&format!("--- {old_label}\n+++ {new_label}\n"));
    for hunk in hunks {
        out.push_str(&format!(
            "@@ -{} +{} @@\n",
            range_label(hunk.old_start, hunk.old_len),
            range_label(hunk.new_start, hunk.new_len)
        ));
        for line in &hunk.lines {
            let prefix = match line.tag {
                LineTag::Context => ' ',
                LineTag::Removed => '-',
                LineTag::Added => '+',
            };
            out.push(prefix);
            out.push_str(&String::from_utf8_lossy(&line.text));
            if !line.text.ends_with(b"\n") {
                out.push_str("\n\\ No newline at end of file\n");
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
    use crate::diff::diff_lines;

    #[test]
    fn renders_like_diff_u() {
        let hunks = diff_lines(b"a\nb\nc\n", b"a\nB\nc\n");
        let text = to_unified("f.txt", &hunks, 3);
        assert_eq!(
            text,
            "--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n"
        );
    }

    #[test]
    fn new_file_uses_dev_null() {
        let hunks = diff_lines(b"", b"x\n");
        let text = to_unified("f", &hunks, 3);
        assert!(text.starts_with("--- /dev/null\n+++ b/f\n@@ -0,0 +1 @@\n+x\n"));
    }

    #[test]
    fn marks_missing_newline() {
        let hunks = diff_lines(b"a\n", b"a");
        let text = to_unified("f", &hunks, 3);
        assert!(text.contains("\\ No newline at end of file"));
    }
}
