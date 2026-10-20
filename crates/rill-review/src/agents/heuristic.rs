//! A reviewer that needs no model: cheap textual checks that catch the
//! most common slips and keep the review pipeline exercisable offline.

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use regex::bytes::Regex;
use rill_types::objects::review::CommentCategory;
use rill_types::RepoPath;

use super::reviewer::{FileDiff, Finding, ReviewContext, Reviewer};
use crate::error::Result;

/// Name the heuristic reviewer votes and comments under.
pub const HEURISTIC_REVIEWER_NAME: &str = "heuristics";

/// Tunables for [`HeuristicReviewer`].
#[derive(Debug, Clone)]
pub struct HeuristicConfig {
    /// An added file with more lines than this is flagged.
    pub max_new_file_lines: usize,
    /// Flag `TODO` / `FIXME` / `XXX` markers in added lines.
    pub flag_todos: bool,
    /// Flag debug printing added to source.
    pub flag_debug_prints: bool,
    /// Flag strings that look like credentials.
    pub flag_secrets: bool,
    /// Flag source changes that touch no test file.
    pub flag_missing_tests: bool,
}

impl Default for HeuristicConfig {
    fn default() -> Self {
        HeuristicConfig {
            max_new_file_lines: 500,
            flag_todos: true,
            flag_debug_prints: true,
            flag_secrets: true,
            flag_missing_tests: true,
        }
    }
}

/// The built-in heuristic reviewer.
#[derive(Debug, Clone, Default)]
pub struct HeuristicReviewer {
    config: HeuristicConfig,
}

static TODO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(TODO|FIXME|XXX)\b").unwrap());
static DEBUG_PRINT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|[^\w.])(println!|eprintln!|dbg!|console\.(log|debug)\(|print\(|fmt\.Println\(|System\.out\.println\(|binding\.pry|breakpoint\(\)|debugger;)")
        .unwrap()
});
static SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)((api[_-]?key|secret|passw(or)?d|token|auth)\s*[:=]\s*["'][^"'\s]{8,}["']|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----|ghp_[A-Za-z0-9]{36}|sk-[A-Za-z0-9]{32,})"#,
    )
    .unwrap()
});

impl HeuristicReviewer {
    /// A reviewer with `config`.
    pub fn new(config: HeuristicConfig) -> Self {
        HeuristicReviewer { config }
    }

    /// Boxed for a [`super::ReviewerPool`].
    pub fn shared(self) -> Arc<dyn Reviewer> {
        Arc::new(self)
    }

    /// Run the checks synchronously; exposed for tests and for callers
    /// that have a context in hand.
    pub fn check(&self, ctx: &ReviewContext) -> Vec<Finding> {
        let mut out = Vec::new();
        for diff in &ctx.diffs {
            self.check_file(diff, &mut out);
        }
        if self.config.flag_missing_tests {
            self.check_tests(ctx, &mut out);
        }
        out
    }

    fn check_file(&self, diff: &FileDiff, out: &mut Vec<Finding>) {
        let is_source = is_source_path(&diff.path);
        for (line_no, text) in diff.added_lines() {
            if self.config.flag_todos && TODO.is_match(text) {
                out.push(Finding::at(
                    diff.path.clone(),
                    line_no,
                    line_no,
                    CommentCategory::Suggestion,
                    55,
                    "Added a TODO/FIXME marker; track it in an intent or resolve it before landing.",
                ));
            }
            if self.config.flag_debug_prints && is_source && DEBUG_PRINT.is_match(text) {
                out.push(Finding::at(
                    diff.path.clone(),
                    line_no,
                    line_no,
                    CommentCategory::Style,
                    65,
                    "Looks like debug output left in; use structured logging or remove it.",
                ));
            }
            if self.config.flag_secrets && SECRET.is_match(text) {
                out.push(Finding::at(
                    diff.path.clone(),
                    line_no,
                    line_no,
                    CommentCategory::Security,
                    85,
                    "This looks like a credential or private key committed to source.",
                ));
            }
        }
        if diff.before.is_none() {
            if let Some(after) = &diff.after {
                let lines = rill_core::diff::split_lines(after).count();
                if lines > self.config.max_new_file_lines {
                    out.push(Finding::file(
                        diff.path.clone(),
                        CommentCategory::Suggestion,
                        50,
                        format!(
                            "New file has {lines} lines (limit {}); consider splitting it.",
                            self.config.max_new_file_lines
                        ),
                    ));
                }
            }
        }
    }

    fn check_tests(&self, ctx: &ReviewContext, out: &mut Vec<Finding>) {
        let touches_source = ctx
            .diffs
            .iter()
            .any(|d| is_source_path(&d.path) && !is_test_path(&d.path));
        let touches_tests = ctx.diffs.iter().any(|d| is_test_path(&d.path));
        if touches_source && !touches_tests {
            out.push(Finding::change(
                CommentCategory::TestGap,
                50,
                "Source files changed but no test file was touched.",
            ));
        }
    }
}

#[async_trait]
impl Reviewer for HeuristicReviewer {
    fn name(&self) -> &str {
        HEURISTIC_REVIEWER_NAME
    }

    async fn review(&self, ctx: Arc<ReviewContext>) -> Result<Vec<Finding>> {
        Ok(self.check(&ctx))
    }
}

const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "go", "py", "js", "jsx", "ts", "tsx", "java", "kt", "swift", "c", "cc", "cpp", "h",
    "hpp", "rb", "cs", "scala", "php", "ex", "exs", "zig",
];

fn is_source_path(path: &RepoPath) -> bool {
    path.extension()
        .map(|e| SOURCE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_test_path(path: &RepoPath) -> bool {
    let name = path.file_name().unwrap_or("").to_ascii_lowercase();
    path.components()
        .any(|c| matches!(c, "tests" | "test" | "__tests__" | "spec"))
        || name.contains("_test.")
        || name.contains(".test.")
        || name.contains("_spec.")
        || name.contains(".spec.")
        || name.starts_with("test_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use rill_core::diff_lines;
    use rill_types::RepoPathBuf;

    fn diff(path: &str, before: &str, after: &str) -> FileDiff {
        FileDiff {
            path: RepoPathBuf::new(path).unwrap(),
            before: (!before.is_empty()).then(|| Bytes::from(before.to_string())),
            after: Some(Bytes::from(after.to_string())),
            hunks: diff_lines(before.as_bytes(), after.as_bytes()),
        }
    }

    #[test]
    fn regexes_match_expected_shapes() {
        assert!(DEBUG_PRINT.is_match(b"    println!(\"x\");"));
        assert!(DEBUG_PRINT.is_match(b"console.log(x)"));
        assert!(!DEBUG_PRINT.is_match(b"tracing::info!(x)"));
        assert!(SECRET.is_match(b"let api_key = \"abcdefghijklmnop\";"));
        assert!(SECRET.is_match(b"AKIAABCDEFGHIJKLMNOP"));
        assert!(!SECRET.is_match(b"let password = read_password();"));
        assert!(TODO.is_match(b"// TODO: later"));
    }

    #[test]
    fn flags_added_lines_only() {
        let r = HeuristicReviewer::default();
        let mut out = Vec::new();
        let d = diff(
            "src/a.rs",
            "fn a() {\n    println!(\"old\");\n}\n",
            "fn a() {\n    println!(\"old\");\n    // TODO fix\n}\n",
        );
        r.check_file(&d, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].category, CommentCategory::Suggestion);
        assert_eq!(out[0].lines, Some((3, 3)));
    }

    #[test]
    fn test_paths() {
        assert!(is_test_path(
            &RepoPathBuf::new("crates/x/tests/it.rs").unwrap()
        ));
        assert!(is_test_path(&RepoPathBuf::new("src/foo_test.go").unwrap()));
        assert!(!is_test_path(&RepoPathBuf::new("src/foo.go").unwrap()));
        assert!(is_source_path(&RepoPathBuf::new("src/foo.go").unwrap()));
        assert!(!is_source_path(&RepoPathBuf::new("README.md").unwrap()));
    }
}
