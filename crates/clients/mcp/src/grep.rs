//! Text search over a root.
//!
//! The server answers `Grep` from its trigram index, so searching a large
//! tree costs one RPC. When the server predates that RPC (`UNIMPLEMENTED`)
//! the search falls back to walking the tree with `list_dir` and reading
//! every candidate file, bounded by [`GrepOptions::max_files`] and
//! [`GrepOptions::max_bytes`]; the result says when a bound was hit.

use globset::{Glob, GlobMatcher};
use regex::bytes::{Regex, RegexBuilder};
use rill_sdk::{GrepQuery, RepoClient, Root};
use rill_types::{EntryKind, RepoPath, RepoPathBuf};
use serde_json::{json, Value};

use crate::error::{ToolError, ToolResult};

/// Tuning for a search.
#[derive(Debug, Clone)]
pub struct GrepOptions {
    /// Case-insensitive matching.
    pub ignore_case: bool,
    /// Only files whose path matches this glob.
    pub glob: Option<String>,
    /// Directory to start from.
    pub path: RepoPathBuf,
    /// Stop after reading this many files.
    pub max_files: usize,
    /// Stop after reading this many bytes.
    pub max_bytes: u64,
    /// Stop after this many matches.
    pub max_matches: usize,
}

impl Default for GrepOptions {
    fn default() -> Self {
        GrepOptions {
            ignore_case: false,
            glob: None,
            path: RepoPathBuf::root(),
            max_files: 2_000,
            max_bytes: 32 * 1024 * 1024,
            max_matches: 1_000,
        }
    }
}

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepMatch {
    /// The file.
    pub path: RepoPathBuf,
    /// One-based line number.
    pub line_no: u32,
    /// The line, without its terminator.
    pub line: String,
}

/// Search results with the limits that were hit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrepResult {
    /// Matches in path, then line order.
    pub matches: Vec<GrepMatch>,
    /// Files whose content was searched.
    pub files_searched: usize,
    /// True when a limit stopped the search early.
    pub truncated: bool,
}

impl GrepResult {
    /// JSON rendering.
    pub fn to_json(&self) -> Value {
        json!({
            "matches": self.matches.iter().map(|m| json!({
                "path": m.path.as_str(),
                "line_no": m.line_no,
                "line": m.line,
            })).collect::<Vec<_>>(),
            "files_searched": self.files_searched,
            "truncated": self.truncated,
        })
    }
}

/// Search `root` for lines matching `pattern` (a Rust regex).
pub async fn grep(
    repo: &RepoClient,
    root: &Root,
    pattern: &str,
    opts: &GrepOptions,
) -> ToolResult<GrepResult> {
    let mut query = GrepQuery::new(pattern)
        .case_insensitive(opts.ignore_case)
        .max_results(opts.max_matches as u32);
    if let Some(glob) = &opts.glob {
        query = query.glob(glob.clone());
    }
    if !opts.path.is_root() {
        query = query.glob(format!("{}/**", opts.path));
    }
    match repo.grep(root, &query).await {
        Ok(found) => {
            return Ok(GrepResult {
                files_searched: found
                    .hits
                    .iter()
                    .map(|h| h.path.as_str())
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                matches: found
                    .hits
                    .into_iter()
                    .map(|h| GrepMatch {
                        path: h.path,
                        line_no: h.line_no,
                        line: h.line,
                    })
                    .collect(),
                truncated: found.truncated,
            });
        }
        Err(rill_sdk::Error::Status {
            code: rill_sdk::Code::Unimplemented,
            ..
        }) => {}
        Err(e) => return Err(e.into()),
    }
    grep_by_walking(repo, root, pattern, opts).await
}

/// Fallback for servers without the `Grep` RPC.
async fn grep_by_walking(
    repo: &RepoClient,
    root: &Root,
    pattern: &str,
    opts: &GrepOptions,
) -> ToolResult<GrepResult> {
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(opts.ignore_case)
        .build()
        .map_err(|e| ToolError::invalid(format!("pattern: {e}")))?;
    let glob = opts
        .glob
        .as_deref()
        .map(|g| Glob::new(g).map(|g| g.compile_matcher()))
        .transpose()
        .map_err(|e| ToolError::invalid(format!("glob: {e}")))?;

    let mut result = GrepResult::default();
    let mut bytes_read = 0u64;
    let mut pending: Vec<RepoPathBuf> = vec![opts.path.clone()];
    while let Some(dir) = pending.pop() {
        let entries = repo.list_dir(root, dir.as_path()).await?;
        // Entries come sorted ascending; push reversed so popping yields
        // them in order and the output stays deterministic.
        for (name, kind) in entries.into_iter().rev() {
            let path = dir.join(&name)?;
            match kind {
                EntryKind::Dir { .. } => pending.push(path),
                EntryKind::File { content, .. } => {
                    if !glob_matches(glob.as_ref(), path.as_path()) {
                        continue;
                    }
                    if result.files_searched >= opts.max_files
                        || bytes_read + content.size() > opts.max_bytes
                    {
                        result.truncated = true;
                        return Ok(result);
                    }
                    let data = repo.get_blob(&content).await?;
                    bytes_read += data.len() as u64;
                    result.files_searched += 1;
                    if rill_core::diff::is_binary(&data) {
                        continue;
                    }
                    if !scan(&regex, &path, &data, opts.max_matches, &mut result) {
                        result.truncated = true;
                        return Ok(result);
                    }
                }
            }
        }
    }
    Ok(result)
}

fn glob_matches(glob: Option<&GlobMatcher>, path: &RepoPath) -> bool {
    match glob {
        None => true,
        Some(g) => g.is_match(path.as_str()) || path.file_name().is_some_and(|n| g.is_match(n)),
    }
}

/// Append matching lines; false when the match limit was reached.
fn scan(regex: &Regex, path: &RepoPath, data: &[u8], max: usize, out: &mut GrepResult) -> bool {
    for (i, line) in data.split(|b| *b == b'\n').enumerate() {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if regex.is_match(line) {
            if out.matches.len() >= max {
                return false;
            }
            out.matches.push(GrepMatch {
                path: path.to_owned_path(),
                line_no: i as u32 + 1,
                line: String::from_utf8_lossy(line).into_owned(),
            });
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_reports_lines_and_limits() {
        let regex = Regex::new("fn").unwrap();
        let path = RepoPathBuf::new("a.rs").unwrap();
        let mut out = GrepResult::default();
        assert!(scan(&regex, &path, b"fn a()\nx\nfn b()\r\n", 10, &mut out));
        assert_eq!(out.matches.len(), 2);
        assert_eq!(out.matches[1].line_no, 3);
        assert_eq!(out.matches[1].line, "fn b()");
        let mut limited = GrepResult::default();
        assert!(!scan(&regex, &path, b"fn\nfn\n", 1, &mut limited));
    }

    #[test]
    fn glob_matches_basename_or_full_path() {
        let g = Glob::new("*.rs").unwrap().compile_matcher();
        assert!(glob_matches(
            Some(&g),
            RepoPathBuf::new("src/a.rs").unwrap().as_path()
        ));
        assert!(!glob_matches(
            Some(&g),
            RepoPathBuf::new("src/a.md").unwrap().as_path()
        ));
    }
}
