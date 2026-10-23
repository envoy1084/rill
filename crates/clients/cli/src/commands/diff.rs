//! `rill diff`: local edits against the base, or two points in history,
//! drawn like `git diff`.

use clap::Args;
use rill_daemon::proto as dpb;
use rill_daemon::{ChangeKind, LocalChange};
use rill_mcp::json;
use rill_sdk::{DiffFormat, DiffOptions, RepoClient, Root};
use rill_types::{RepoPathBuf, Tree};
use serde_json::{json as j, Value};

use super::status::parse_paths;
use crate::commands::root_arg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{FileStatus, PatchFile, PatchMode};
use crate::Result;

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
    #[arg(long, short = 'U', default_value_t = 3)]
    pub context: u32,
    /// A diffstat instead of the patch.
    #[arg(long, conflicts_with = "name_status")]
    pub stat: bool,
    /// Paths with a status letter instead of the patch.
    #[arg(long)]
    pub name_status: bool,
    /// Only these paths or prefixes.
    pub paths: Vec<String>,
}

impl DiffArgs {
    fn mode(&self) -> PatchMode {
        match (self.stat, self.name_status) {
            (true, _) => PatchMode::Stat,
            (_, true) => PatchMode::NameStatus,
            _ => PatchMode::Full,
        }
    }
}

/// Run `rill diff`.
pub async fn diff(ctx: &mut Ctx<'_>, args: DiffArgs) -> Result<()> {
    let paths = parse_paths(&args.paths)?;
    if let Some(target) = &args.target {
        let target = target.clone();
        return server_diff(ctx, &args, &target, paths).await;
    }
    if ctx.find_checkout().is_none() {
        return Err(CliError::usage(
            "not inside a checkout: pass --target <root> to diff two points in history",
        ));
    }
    local_diff(ctx, &args, paths).await
}

/// The diff of two roots, as files to print.
pub(crate) async fn root_diff(
    repo: &RepoClient,
    base: &Root,
    target: &Root,
    paths: Vec<RepoPathBuf>,
    context: u32,
    format: DiffFormat,
) -> Result<(rill_sdk::DiffResult, Vec<PatchFile>)> {
    let result = repo
        .diff_with(
            base,
            target,
            DiffOptions {
                paths,
                format,
                context_lines: context,
                limit: 0,
            },
        )
        .await?;
    let files = result
        .files
        .iter()
        .map(|f| PatchFile {
            path: f.path.as_str().to_string(),
            status: if f.conflict.is_some() {
                FileStatus::Conflict
            } else if f.is_added() {
                FileStatus::Added
            } else if f.is_deleted() {
                FileStatus::Deleted
            } else {
                FileStatus::Modified
            },
            binary: f.binary,
            unified: f.unified.clone().unwrap_or_default(),
        })
        .collect();
    Ok((result, files))
}

async fn server_diff(
    ctx: &mut Ctx<'_>,
    args: &DiffArgs,
    target: &str,
    paths: Vec<RepoPathBuf>,
) -> Result<()> {
    let base = root_arg(args.base.as_deref())?;
    let target = root_arg(Some(target))?;
    let repo = ctx.repo()?;
    let format = if ctx.json() {
        DiffFormat::Structured
    } else {
        DiffFormat::Unified
    };
    let (result, files) = root_diff(&repo, &base, &target, paths, args.context, format).await?;
    let mut value = json::diff_result(&result);
    value["base"] = Value::String(base.to_string());
    value["target"] = Value::String(target.to_string());
    let mode = args.mode();
    ctx.emit(value, |out| {
        out.patch(&files, mode)?;
        if result.truncated {
            out.line(out.warn("(diff truncated by the server)"))?;
        }
        Ok(())
    })
}

async fn local_diff(ctx: &mut Ctx<'_>, args: &DiffArgs, prefixes: Vec<RepoPathBuf>) -> Result<()> {
    let dir = ctx.require_checkout_dir()?;
    let mount = ctx.mount().await?;
    let base = match args.base.as_deref() {
        Some(b) => root_arg(Some(b))?,
        None if mount.base_version.is_empty() => Root::Tree(Tree::empty_hash()),
        None => Root::Version(mount.base_version.parse()?),
    };
    let reply = ctx
        .daemon()
        .await?
        .get_status(dpb::GetStatusRequest {
            name: mount.name.clone(),
            prefixes: prefixes.iter().map(|p| p.as_str().to_string()).collect(),
        })
        .await?;
    let changes = rill_daemon::client::local_changes(&reply.changes);
    let repo = ctx.repo()?;
    let mut files = Vec::with_capacity(changes.len());
    for change in &changes {
        files.push(diff_one(&repo, &dir, &base, change, args.context).await?);
    }
    let mode = args.mode();
    ctx.emit(
        j!({
            "base": base.to_string(),
            "files": files.iter().zip(&changes).map(|(f, c)| j!({
                "path": f.path,
                "status": json::enum_name(&c.kind),
                "binary": f.binary,
                "unified": f.unified,
            })).collect::<Vec<_>>(),
        }),
        |out| {
            if files.is_empty() {
                return out.line(out.dim("no local changes"));
            }
            out.patch(&files, mode)
        },
    )
}

async fn diff_one(
    repo: &RepoClient,
    dir: &std::path::Path,
    base: &Root,
    change: &LocalChange,
    context: u32,
) -> Result<PatchFile> {
    let before = match change.kind {
        ChangeKind::Added => Vec::new(),
        _ => repo
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
        ChangeKind::Deleted => Vec::new(),
        _ => {
            let mut fs_path = dir.to_path_buf();
            fs_path.extend(change.path.components());
            tokio::task::spawn_blocking(move || std::fs::read(fs_path))
                .await
                .map_err(|e| CliError::Other(anyhow::anyhow!("read task failed: {e}")))??
        }
    };
    let binary = rill_core::diff::is_binary(&before) || rill_core::diff::is_binary(&after);
    let unified = if binary {
        String::new()
    } else {
        let hunks = rill_core::diff::diff_lines_with_context(&before, &after, context as usize);
        String::from_utf8_lossy(&rill_core::diff::to_unified(change.path.as_str(), &hunks))
            .into_owned()
    };
    Ok(PatchFile {
        path: change.path.as_str().to_string(),
        status: match change.kind {
            ChangeKind::Added => FileStatus::Added,
            ChangeKind::Deleted => FileStatus::Deleted,
            ChangeKind::Conflict => FileStatus::Conflict,
            ChangeKind::Modified => FileStatus::Modified,
        },
        binary,
        unified,
    })
}
