//! Read-only repository tools: info, trees, files, history, blame, diff
//! and grep.

use rill_sdk::{DiffFormat, DiffOptions, LogStart, Root};
use rill_types::ChangeId;
use serde_json::{json, Value};

use super::args::Args;
use super::schema::{boolean, integer, root, string, string_array, Props};
use super::{ToolContext, ToolDef};
use crate::error::{ToolError, ToolResult};
use crate::grep::{grep, GrepOptions};
use crate::json;

/// Tool definitions in this group.
pub fn definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "rill_repo_info",
            description: "Describe a repository: id, name, description and trunk head sequence.",
            input_schema: Props::repo().build(),
        },
        ToolDef {
            name: "rill_list_dir",
            description: "List the entries of a directory at a root (files with size and kind, directories with summaries).",
            input_schema: Props::repo()
                .opt("path", string("Directory path; empty or omitted for the repository root"))
                .opt("at", root("Root to read from (default trunk)"))
                .build(),
        },
        ToolDef {
            name: "rill_read_file",
            description: "Read a file at a root. Returns UTF-8 text when valid, hex otherwise.",
            input_schema: Props::repo()
                .req("path", string("File path"))
                .opt("at", root("Root to read from (default trunk)"))
                .build(),
        },
        ToolDef {
            name: "rill_log",
            description: "Walk history newest-first from a root, optionally restricted to versions touching a path.",
            input_schema: Props::repo()
                .opt("start", root("Where to start (default trunk)"))
                .opt("limit", integer("Maximum entries (default 20)"))
                .opt("path", string("Only versions that touched this path"))
                .opt("first_parent", boolean("Follow only first parents"))
                .opt("page_token", string("Continue from a previous page's next_page_token"))
                .build(),
        },
        ToolDef {
            name: "rill_blame",
            description: "Line-by-line attribution of a file at a root.",
            input_schema: Props::repo()
                .req("path", string("File path"))
                .opt("at", root("Root to blame at (default trunk)"))
                .build(),
        },
        ToolDef {
            name: "rill_diff",
            description: "Diff two roots, or a change's current version against its parent. Returns structured hunks or unified text per file.",
            input_schema: Props::repo()
                .opt("base", root("Base root (default trunk)"))
                .opt("target", root("Target root; required unless change_id is given"))
                .opt("change_id", string("Diff this change's current version against its first parent"))
                .opt("paths", string_array("Restrict to these paths or prefixes"))
                .opt("unified", boolean("Return unified diff text instead of structured hunks"))
                .opt("context_lines", integer("Context lines per hunk (default 3)"))
                .opt("limit", integer("Maximum files; 0 lets the server choose"))
                .build(),
        },
        ToolDef {
            name: "rill_grep",
            description: "Search file contents at a root with a regular expression. Served from the server trigram index (falls back to walking the tree on servers without the Grep RPC); bounded by result limits and reports truncation.",
            input_schema: Props::repo()
                .req("pattern", string("Rust regular expression"))
                .opt("at", root("Root to search (default trunk)"))
                .opt("path", string("Directory to search under (default root)"))
                .opt("glob", string("Only files matching this glob, e.g. `*.rs`"))
                .opt("ignore_case", boolean("Case-insensitive matching"))
                .opt("max_matches", integer("Stop after this many matches (default 1000)"))
                .build(),
        },
    ]
}

/// Dispatch a call in this group.
pub async fn call(ctx: &ToolContext, name: &str, args: &Args) -> Option<ToolResult<Value>> {
    Some(match name {
        "rill_repo_info" => info(ctx, args).await,
        "rill_list_dir" => list_dir(ctx, args).await,
        "rill_read_file" => read_file(ctx, args).await,
        "rill_log" => log(ctx, args).await,
        "rill_blame" => blame(ctx, args).await,
        "rill_diff" => diff(ctx, args).await,
        "rill_grep" => grep_tool(ctx, args).await,
        _ => return None,
    })
}

async fn info(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    Ok(json::repo_info(&ctx.repo(args)?.info().await?))
}

async fn list_dir(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let root = args.root_or_trunk("at")?;
    let path = args.path_or_root("path")?;
    let listing = repo.list_dir_full(&root, path.as_path()).await?;
    Ok(json!({
        "root": root.to_string(),
        "path": path.as_str(),
        "tree_hash": listing.tree_hash.to_hex(),
        "entries": listing.entries.iter().map(|(n, k)| json::dir_entry(n, k)).collect::<Vec<_>>(),
    }))
}

async fn read_file(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let root = args.root_or_trunk("at")?;
    let path = args.path("path")?;
    let data = repo.read_file(&root, path.as_path()).await?;
    let mut out = json::bytes(&data);
    out["path"] = Value::String(path.into_string());
    out["root"] = Value::String(root.to_string());
    Ok(out)
}

async fn log(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let start: LogStart = args.root_or_trunk("start")?.into();
    let limit = args.limit("limit", 20)?;
    let path = args
        .opt_str("path")
        .map(rill_types::RepoPathBuf::new)
        .transpose()?;
    let first_parent = args.bool_or("first_parent", false)?;
    let page_token = args.opt_string("page_token")?;
    let page = repo
        .log_page(
            start,
            limit,
            path.as_deref(),
            first_parent,
            page_token.as_deref(),
        )
        .await?;
    Ok(json::log_page(&page))
}

async fn blame(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let root = args.root_or_trunk("at")?;
    let path = args.path("path")?;
    let hash = repo.resolve_root(&root).await?;
    let lines = repo.blame(hash, path.as_path()).await?;
    Ok(json!({
        "path": path.as_str(),
        "version": hash.to_hex(),
        "lines": lines.iter().map(json::blame_line).collect::<Vec<_>>(),
    }))
}

async fn diff(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let (base, target) = match args.opt_parse::<ChangeId>("change_id")? {
        Some(id) => {
            let info = repo.current_version(id).await?;
            let parent = info.parents.first().copied().ok_or_else(|| {
                ToolError::invalid(format!("change {id} has no parent to diff against"))
            })?;
            (Root::Version(parent), Root::Version(info.hash))
        }
        None => {
            let target = args
                .opt_parse::<Root>("target")?
                .ok_or_else(|| ToolError::invalid("`target` or `change_id` is required"))?;
            (args.root_or_trunk("base")?, target)
        }
    };
    let opts = DiffOptions {
        paths: args.opt_paths("paths")?,
        format: if args.bool_or("unified", false)? {
            DiffFormat::Unified
        } else {
            DiffFormat::Structured
        },
        context_lines: args.limit("context_lines", 3)?,
        limit: args.limit("limit", 0)?,
    };
    let result = repo.diff_with(&base, &target, opts).await?;
    let mut out = json::diff_result(&result);
    out["base"] = Value::String(base.to_string());
    out["target"] = Value::String(target.to_string());
    Ok(out)
}

async fn grep_tool(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let root = args.root_or_trunk("at")?;
    let opts = GrepOptions {
        ignore_case: args.bool_or("ignore_case", false)?,
        glob: args.opt_string("glob")?,
        path: args.path_or_root("path")?,
        max_matches: args.limit("max_matches", 1_000)? as usize,
        ..GrepOptions::default()
    };
    let result = grep(&repo, &root, args.str("pattern")?, &opts).await?;
    let mut out = result.to_json();
    out["root"] = Value::String(root.to_string());
    Ok(out)
}
