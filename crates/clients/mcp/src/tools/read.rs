//! `rill_read`: file content, directory listings, history, blame and
//! diffs, batched over paths and fitted to the response budget.

use rill_sdk::{DiffFormat, DiffOptions, RepoClient, Root};
use rill_types::{RepoPath, RepoPathBuf, Tree};
use serde_json::{json, Value};

use super::args::Args;
use super::budget::{self, fit_lines, Budget, Cursor};
use super::root::read_root;
use super::schema::{integer, root, string_array, string_enum, Props};
use super::{ToolContext, ToolDef};
use crate::error::{ToolError, ToolResult};
use crate::json;

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_read",
        description: "Read the repository: file `content` (with line ranges), `list` a directory, path `history`, `blame`, or the `diff` of a change. Reads the session's change by default (so you see your own edits), else trunk; pass `at` to pick another root. Large results are cut to `max_tokens` with a next_cursor.",
        input_schema: budget::props(
            Props::repo()
                .opt("paths", string_array("Paths to read; directories for `list`, one path for `history`/`blame`, a filter for `diff`. Default: the repository root"))
                .opt("mode", string_enum("What to read (default content)", &["content", "list", "history", "blame", "diff"]))
                .opt("at", root("Root to read"))
                .change()
                .opt("start_line", integer("content: first line, 1-based"))
                .opt("end_line", integer("content: last line, inclusive"))
                .opt("limit", integer("history: entries per page (default 20)")),
        )
        .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let mut paths = args.opt_paths("paths")?;
    if paths.is_empty() {
        paths.push(RepoPathBuf::root());
    }
    let mut budget = Budget::from_args(args)?;
    let mode = args.opt_str("mode").unwrap_or("content");
    if mode == "diff" {
        return diff(ctx, &repo, args, paths, &mut budget).await;
    }
    let root = read_root(ctx, &repo, args).await?;
    let cursor = || Cursor::from_args(args);
    let mut out = match mode {
        "content" => content(&repo, &root, &paths, args, cursor()?, &mut budget).await?,
        "list" => list(&repo, &root, &paths, cursor()?, &mut budget).await?,
        "history" => history(&repo, &root, single(&paths)?, args).await?,
        "blame" => blame(&repo, &root, single(&paths)?, cursor()?, &mut budget).await?,
        other => return Err(ToolError::invalid(format!("unknown mode {other:?}"))),
    };
    out["root"] = json!(root.to_string());
    Ok(out)
}

fn single(paths: &[RepoPathBuf]) -> ToolResult<&RepoPath> {
    match paths {
        [one] if !one.is_root() => Ok(one.as_path()),
        _ => Err(ToolError::invalid("this mode reads exactly one file path")),
    }
}

async fn content(
    repo: &RepoClient,
    root: &Root,
    paths: &[RepoPathBuf],
    args: &Args,
    cursor: Cursor,
    budget: &mut Budget,
) -> ToolResult<Value> {
    let start_line = args.opt_u64("start_line")?.unwrap_or(1).max(1) as usize;
    let end_line = args.opt_u64("end_line")?.map(|e| e as usize);
    let mut files = Vec::new();
    let mut next = None;
    for (i, path) in paths.iter().enumerate().skip(cursor.item) {
        if budget.is_spent() && !files.is_empty() {
            next = Some(Cursor { item: i, inner: 0 });
            break;
        }
        let data = match repo.read_file(root, path.as_path()).await {
            Ok(data) => data,
            Err(e) if e.is_not_found() => {
                files.push(json!({ "path": path.as_str(), "error": "not_found" }));
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        if rill_core::diff::is_binary(&data) {
            files.push(json!({ "path": path.as_str(), "binary": true, "size": data.len() }));
            continue;
        }
        let text = String::from_utf8_lossy(&data);
        let lines: Vec<&str> = text.lines().collect();
        let from = match (i == cursor.item, cursor.inner) {
            (true, n) if n > 0 => n,
            _ => start_line,
        };
        let last = end_line.unwrap_or(lines.len()).min(lines.len());
        let wanted = lines.get(from - 1..last).unwrap_or(&[]);
        let shown = fit_lines(wanted, budget);
        let mut file = json!({
            "path": path.as_str(),
            "start_line": from,
            "end_line": from + shown.saturating_sub(1),
            "total_lines": lines.len(),
            "text": wanted[..shown].join("\n"),
        });
        if shown < wanted.len() {
            file["truncated"] = json!(true);
            files.push(file);
            next = Some(Cursor {
                item: i,
                inner: from + shown,
            });
            break;
        }
        files.push(file);
    }
    let mut out = json!({ "files": files });
    budget::finish(&mut out, next);
    Ok(out)
}

async fn list(
    repo: &RepoClient,
    root: &Root,
    paths: &[RepoPathBuf],
    cursor: Cursor,
    budget: &mut Budget,
) -> ToolResult<Value> {
    let mut dirs = Vec::new();
    let mut next = None;
    for (i, path) in paths.iter().enumerate().skip(cursor.item) {
        let listing = repo.list_dir_full(root, path.as_path()).await?;
        let entries: Vec<Value> = listing
            .entries
            .iter()
            .map(|(name, kind)| json::dir_entry(name, kind))
            .collect();
        let start = if i == cursor.item { cursor.inner } else { 0 };
        let (shown, cut) = budget::take(&entries, start, budget);
        dirs.push(json!({ "path": path.as_str(), "entries": shown }));
        if let Some(cut) = cut {
            next = Some(Cursor {
                item: i,
                inner: cut.item,
            });
            break;
        }
    }
    let mut out = json!({ "dirs": dirs });
    budget::finish(&mut out, next);
    Ok(out)
}

async fn history(
    repo: &RepoClient,
    root: &Root,
    path: &RepoPath,
    args: &Args,
) -> ToolResult<Value> {
    let page_token = args.opt_string("cursor")?;
    let page = repo
        .path_history(
            path,
            root.clone().into(),
            args.limit("limit", 20)?,
            page_token.as_deref(),
        )
        .await?;
    let mut out = json::path_history_page(&page);
    out["path"] = json!(path.as_str());
    out["truncated"] = json!(page.next_page_token.is_some());
    if let Some(token) = &page.next_page_token {
        out["next_cursor"] = json!(token);
    }
    Ok(out)
}

async fn blame(
    repo: &RepoClient,
    root: &Root,
    path: &RepoPath,
    cursor: Cursor,
    budget: &mut Budget,
) -> ToolResult<Value> {
    let hash = repo.resolve_root(root).await?;
    let lines: Vec<Value> = repo
        .blame(hash, path)
        .await?
        .iter()
        .map(json::blame_line)
        .collect();
    let (shown, next) = budget::take(&lines, cursor.item, budget);
    let mut out = json!({ "path": path.as_str(), "version": hash.to_hex(), "lines": shown });
    budget::finish(&mut out, next);
    Ok(out)
}

async fn diff(
    ctx: &ToolContext,
    repo: &RepoClient,
    args: &Args,
    paths: Vec<RepoPathBuf>,
    budget: &mut Budget,
) -> ToolResult<Value> {
    let (base, target) = match (args.opt_parse::<Root>("at")?, ctx.opt_change(args)?) {
        (Some(at), _) => (Root::trunk(), at),
        (None, Some(id)) => {
            let info = repo.current_version(id).await?;
            let base = match info.parents.first() {
                Some(parent) => Root::Version(*parent),
                None => Root::Tree(Tree::empty_hash()),
            };
            (base, Root::Version(info.hash))
        }
        (None, None) => return Err(ToolError::NoChange),
    };
    let opts = DiffOptions {
        paths: paths.into_iter().filter(|p| !p.is_root()).collect(),
        format: DiffFormat::Unified,
        context_lines: 3,
        limit: 0,
    };
    let result = repo.diff_with(&base, &target, opts).await?;
    let files: Vec<Value> = result
        .files
        .iter()
        .map(|f| {
            let full = json::file_diff(f);
            json!({
                "path": full["path"],
                "status": full["status"],
                "binary": f.binary,
                "unified": f.unified,
            })
        })
        .collect();
    let (shown, next) = budget::take(&files, Cursor::from_args(args)?.item, budget);
    let mut out = json!({
        "base": base.to_string(),
        "target": target.to_string(),
        "files": shown,
    });
    budget::finish(&mut out, next);
    if result.truncated {
        out["truncated"] = json!(true);
    }
    Ok(out)
}
