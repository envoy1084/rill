//! `rill status` and `rill diff`.

use clap::Args;
use rill_mcp::json;
use rill_sdk::{Checkout, DiffFormat, DiffOptions, LocalChange, LocalChangeKind, Root};
use rill_types::{RepoPath, RepoPathBuf};
use serde_json::{json as j, Value};

use crate::commands::root_arg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Out;
use crate::Result;

/// `rill status`.
#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Only paths under these prefixes.
    pub paths: Vec<String>,
}

/// `rill diff`.
#[derive(Args, Debug)]
pub struct DiffArgs {
    /// Base root (default: the checkout's base, or trunk with --target).
    #[arg(long, value_name = "ROOT")]
    pub base: Option<String>,
    /// Diff this root against the base on the server instead of local edits.
    #[arg(long, value_name = "ROOT")]
    pub target: Option<String>,
    /// Context lines around each hunk.
    #[arg(long, default_value_t = 3)]
    pub context: u32,
    /// Only these paths or prefixes.
    pub paths: Vec<String>,
}

fn parse_paths(paths: &[String]) -> Result<Vec<RepoPathBuf>> {
    paths
        .iter()
        .map(|p| RepoPathBuf::new(p).map_err(Into::into))
        .collect()
}

fn under(path: &RepoPath, prefixes: &[RepoPathBuf]) -> bool {
    prefixes.is_empty() || prefixes.iter().any(|p| path.starts_with(p))
}

fn kind_letter(kind: LocalChangeKind) -> &'static str {
    match kind {
        LocalChangeKind::Added => "A",
        LocalChangeKind::Modified => "M",
        LocalChangeKind::Deleted => "D",
        LocalChangeKind::Conflict => "C",
    }
}

/// Run `rill status`.
pub async fn status(ctx: &mut Ctx<'_>, args: StatusArgs) -> Result<()> {
    let prefixes = parse_paths(&args.paths)?;
    let checkout = ctx.checkout().await?;
    let changes: Vec<LocalChange> = checkout
        .status()
        .await?
        .into_iter()
        .filter(|c| under(&c.path, &prefixes))
        .collect();
    let meta = checkout.meta().clone();
    let conflicts = changes
        .iter()
        .filter(|c| c.kind == LocalChangeKind::Conflict)
        .count();
    ctx.emit(
        j!({
            "repo": meta.repo,
            "base": meta.base.to_string(),
            "base_version": meta.base_version.map(|h| h.to_hex()),
            "working_change": meta.working_change.map(|c| c.to_string()),
            "workspace": meta.workspace.map(|w| w.to_string()),
            "changes": changes.iter().map(json::local_change).collect::<Vec<_>>(),
            "conflicts": conflicts,
        }),
        |out| {
            out.line(format!(
                "{} at {}{}",
                out.bold(&meta.repo),
                meta.base,
                meta.base_version
                    .map(|h| format!(" ({})", crate::output::short(&h)))
                    .unwrap_or_default()
            ))?;
            if let Some(c) = meta.working_change {
                out.line(format!("working change {}", out.id(&c.to_string())))?;
            }
            if changes.is_empty() {
                return out.line(out.dim("no local changes"));
            }
            for c in &changes {
                let letter = kind_letter(c.kind);
                let styled = match c.kind {
                    LocalChangeKind::Added => out.good(letter),
                    LocalChangeKind::Modified => out.warn(letter),
                    LocalChangeKind::Deleted => out.bad(letter),
                    LocalChangeKind::Conflict => out.bad(letter),
                };
                let suffix = if c.kind == LocalChangeKind::Conflict {
                    out.bad("  (conflict markers; edit then `rill resolve`)")
                } else {
                    String::new()
                };
                out.line(format!("{styled} {}{suffix}", c.path))?;
            }
            if conflicts > 0 {
                out.line(out.bad(&format!("{conflicts} conflicted path(s)")))?;
            }
            Ok(())
        },
    )
}

/// Run `rill diff`.
pub async fn diff(ctx: &mut Ctx<'_>, args: DiffArgs) -> Result<()> {
    let paths = parse_paths(&args.paths)?;
    if let Some(target) = &args.target {
        return server_diff(ctx, args.base.as_deref(), target, paths, args.context).await;
    }
    if ctx.find_checkout().is_none() {
        return Err(CliError::usage(
            "not inside a checkout: pass --target <root> to diff two points in history",
        ));
    }
    local_diff(ctx, args.base.as_deref(), paths, args.context).await
}

async fn server_diff(
    ctx: &mut Ctx<'_>,
    base: Option<&str>,
    target: &str,
    paths: Vec<RepoPathBuf>,
    context: u32,
) -> Result<()> {
    let base = root_arg(base)?;
    let target = root_arg(Some(target))?;
    let repo = ctx.repo()?;
    let format = if ctx.json() {
        DiffFormat::Structured
    } else {
        DiffFormat::Unified
    };
    let result = repo
        .diff_with(
            &base,
            &target,
            DiffOptions {
                paths,
                format,
                context_lines: context,
                limit: 0,
            },
        )
        .await?;
    let mut value = json::diff_result(&result);
    value["base"] = Value::String(base.to_string());
    value["target"] = Value::String(target.to_string());
    ctx.emit(value, |out| {
        for f in &result.files {
            print_file_header(out, f.path.as_str(), f.is_added(), f.is_deleted(), f.binary)?;
            if let Some(text) = &f.unified {
                out.raw(text.as_bytes())?;
            }
        }
        if result.truncated {
            out.line(out.warn("(diff truncated by the server)"))?;
        }
        Ok(())
    })
}

fn print_file_header(
    out: &mut Out<'_>,
    path: &str,
    added: bool,
    deleted: bool,
    binary: bool,
) -> std::io::Result<()> {
    let tag = if added {
        out.good("added")
    } else if deleted {
        out.bad("deleted")
    } else {
        out.warn("modified")
    };
    let note = if binary { " (binary)" } else { "" };
    out.line(format!("{} {path}{note}", out.bold(&tag)))
}

/// One locally edited file rendered as a unified diff against the base.
struct LocalFileDiff {
    path: RepoPathBuf,
    kind: LocalChangeKind,
    binary: bool,
    unified: Vec<u8>,
}

async fn local_diff(
    ctx: &mut Ctx<'_>,
    base: Option<&str>,
    prefixes: Vec<RepoPathBuf>,
    context: u32,
) -> Result<()> {
    let checkout = ctx.checkout().await?;
    let base = match base {
        Some(b) => root_arg(Some(b))?,
        None => checkout.base().clone(),
    };
    let changes: Vec<LocalChange> = checkout
        .status()
        .await?
        .into_iter()
        .filter(|c| under(&c.path, &prefixes))
        .collect();
    let mut files = Vec::with_capacity(changes.len());
    for change in &changes {
        files.push(diff_one(&checkout, &base, change, context).await?);
    }
    ctx.emit(
        j!({
            "base": base.to_string(),
            "files": files.iter().map(|f| j!({
                "path": f.path.as_str(),
                "status": json::enum_name(&f.kind),
                "binary": f.binary,
                "unified": String::from_utf8_lossy(&f.unified),
            })).collect::<Vec<_>>(),
        }),
        |out| {
            if files.is_empty() {
                return out.line(out.dim("no local changes"));
            }
            for f in &files {
                print_file_header(
                    out,
                    f.path.as_str(),
                    f.kind == LocalChangeKind::Added,
                    f.kind == LocalChangeKind::Deleted,
                    f.binary,
                )?;
                out.raw(&f.unified)?;
            }
            Ok(())
        },
    )
}

async fn diff_one(
    checkout: &Checkout,
    base: &Root,
    change: &LocalChange,
    context: u32,
) -> Result<LocalFileDiff> {
    let before = match change.kind {
        LocalChangeKind::Added => Vec::new(),
        _ => checkout
            .repo()
            .read_file(base, change.path.as_path())
            .await
            .map(|b| b.to_vec())
            .or_else(|e| {
                if e.is_not_found() {
                    Ok(Vec::new())
                } else {
                    Err(e)
                }
            })?,
    };
    let after = match change.kind {
        LocalChangeKind::Deleted => Vec::new(),
        _ => {
            let fs_path = checkout.fs_path(change.path.as_path());
            tokio::task::spawn_blocking(move || std::fs::read(fs_path))
                .await
                .map_err(|e| CliError::Other(anyhow::anyhow!("read task failed: {e}")))??
        }
    };
    let binary = rill_core::diff::is_binary(&before) || rill_core::diff::is_binary(&after);
    let unified = if binary {
        Vec::new()
    } else {
        let hunks = rill_core::diff::diff_lines_with_context(&before, &after, context as usize);
        rill_core::diff::to_unified(change.path.as_str(), &hunks)
    };
    Ok(LocalFileDiff {
        path: change.path.clone(),
        kind: change.kind,
        binary,
        unified,
    })
}
