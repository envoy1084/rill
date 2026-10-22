//! `rill stack`: the working change and its unlanded ancestors as one
//! unit: show, sync, submit, land.

use std::time::Duration;

use clap::{Args, Subcommand};
use rill_mcp::json;
use rill_sdk::{RepoClient, Root};
use rill_types::{ChangeId, ChangeInfo};
use serde_json::json as j;

use crate::commands::history::log_row;
use crate::commands::{change_ref, root_arg};
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Table;
use crate::Result;

/// `rill stack <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum StackCmd {
    /// Show the stack: the working change and its ancestors up to trunk.
    Show(StackArgs),
    /// Rebase the stack bottom-up onto a root (default trunk).
    Sync(StackSyncArgs),
    /// Submit every change in the stack for review and declare the stack.
    Submit(StackArgs),
    /// Land the stack as one unit: all or nothing, or `--prefix`.
    Land(StackLandArgs),
}

/// Options shared by stack commands.
#[derive(Args, Debug)]
pub struct StackArgs {
    /// Top of the stack (default: the checkout's working change).
    #[arg(long, value_name = "CHANGE")]
    pub top: Option<String>,
}

/// `rill stack sync`.
#[derive(Args, Debug)]
pub struct StackSyncArgs {
    /// Top of the stack (default: the checkout's working change).
    #[arg(long, value_name = "CHANGE")]
    pub top: Option<String>,
    /// Root to rebase onto (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub onto: Option<String>,
}

/// `rill stack land`.
#[derive(Args, Debug)]
pub struct StackLandArgs {
    /// Top of the stack (default: the checkout's working change).
    #[arg(long, value_name = "CHANGE")]
    pub top: Option<String>,
    /// Wait for each landing to finish (always on; kept for symmetry with `rill land`).
    #[arg(long)]
    pub wait: bool,
    /// Seconds to wait for each landing.
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
    /// Land as many changes from the bottom as can land, rather than all
    /// or none.
    #[arg(long)]
    pub prefix: bool,
    /// Land on this release (`release/<name>`) instead of trunk.
    #[arg(long, value_name = "REF")]
    pub onto: Option<String>,
}

/// Run a stack subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: StackCmd) -> Result<()> {
    match cmd {
        StackCmd::Show(args) => show(ctx, args).await,
        StackCmd::Sync(args) => sync(ctx, args).await,
        StackCmd::Submit(args) => submit(ctx, args).await,
        StackCmd::Land(args) => land(ctx, args).await,
    }
}

/// The top of the stack: `--top`, else the checkout's working change.
async fn top(ctx: &mut Ctx<'_>, top: Option<&str>, repo: &RepoClient) -> Result<ChangeId> {
    if let Some(t) = top {
        return change_ref(t)?.id(repo).await;
    }
    ctx.checkout_meta()
        .and_then(|m| m.working_change)
        .ok_or_else(|| CliError::usage("no working change in this checkout; pass --top <change>"))
}

/// Walk first parents from `top` until a landed version, collecting the
/// unlanded changes bottom-up.
pub async fn collect(repo: &RepoClient, top: ChangeId) -> Result<Vec<ChangeInfo>> {
    let mut chain = Vec::new();
    let mut info = repo.current_version(top).await?;
    // A bound keeps a corrupt or enormous unlanded chain from spinning.
    for _ in 0..10_000 {
        if info.trunk_seq.is_some() {
            break;
        }
        let parent = info.parents.first().copied();
        chain.push(info);
        match parent {
            Some(p) => info = repo.change_info(p).await?,
            None => break,
        }
    }
    chain.reverse();
    Ok(chain)
}

fn ids(chain: &[ChangeInfo]) -> Vec<ChangeId> {
    chain.iter().map(|c| c.change_id).collect()
}

async fn show(ctx: &mut Ctx<'_>, args: StackArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let top = top(ctx, args.top.as_deref(), &repo).await?;
    let chain = collect(&repo, top).await?;
    ctx.emit(
        j!({ "top": top.to_string(), "changes": chain.iter().map(json::change_info).collect::<Vec<_>>() }),
        |out| {
            if chain.is_empty() {
                return out.line(out.dim("stack is empty (top is landed)"));
            }
            let mut table = Table::new(&["#", "CHANGE", "VERSION", "SEQ", "AUTHOR", "WHEN", "SUMMARY"]);
            for (i, c) in chain.iter().enumerate().rev() {
                let mut row = vec![(i + 1).to_string(), out.id(&c.change_id.to_string())];
                row.extend(log_row(out, c));
                table.row(row);
            }
            out.table(table)
        },
    )
}

async fn sync(ctx: &mut Ctx<'_>, args: StackSyncArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let top = top(ctx, args.top.as_deref(), &repo).await?;
    let onto: Root = root_arg(args.onto.as_deref())?;
    let chain = collect(&repo, top).await?;
    let result = repo.stack().sync_stack(&ids(&chain), &onto).await?;
    let clean = result.is_clean();
    ctx.emit(json::sync_result(&result), |out| {
        for c in &result.changes {
            let status = if c.conflicts.is_empty() {
                out.good("ok")
            } else {
                out.bad(&format!(
                    "conflicts: {}",
                    c.conflicts
                        .iter()
                        .map(|p| p.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            };
            out.line(format!(
                "{} {} -> {}  {status}",
                out.id(&c.change_id.to_string()),
                crate::output::short(&c.from),
                crate::output::short(&c.to)
            ))?;
        }
        Ok(())
    })?;
    if clean {
        Ok(())
    } else {
        Err(CliError::conflict("stack sync left conflicts; resolve them with `rill resolve` in a checkout or the resolve API"))
    }
}

async fn submit(ctx: &mut Ctx<'_>, args: StackArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let top = top(ctx, args.top.as_deref(), &repo).await?;
    let chain = collect(&repo, top).await?;
    if chain.is_empty() {
        return Err(CliError::usage("nothing to submit: the stack is empty"));
    }
    let states = repo.stack().submit_stack(&ids(&chain)).await?;
    ctx.emit(
        j!({ "reviews": states.iter().map(json::review_state).collect::<Vec<_>>() }),
        |out| {
            for s in &states {
                out.line(format!(
                    "submitted {} ({:?})",
                    out.id(&s.change_id.to_string()),
                    s.status
                ))?;
            }
            Ok(())
        },
    )
}

async fn land(ctx: &mut Ctx<'_>, args: StackLandArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let top = top(ctx, args.top.as_deref(), &repo).await?;
    let chain = collect(&repo, top).await?;
    if chain.is_empty() {
        return Err(CliError::usage("nothing to land: the stack is empty"));
    }
    let landing = repo
        .stack()
        .land_stack(
            &ids(&chain),
            args.prefix,
            args.onto.as_deref(),
            Duration::from_secs(args.timeout),
        )
        .await?;
    ctx.emit(
        j!({
            "landed": landing.landed.iter().map(json::landed_change).collect::<Vec<_>>(),
            "unlanded": landing.unlanded.iter().map(|u| j!({
                "change_id": u.change_id.to_string(),
                "message": u.message,
            })).collect::<Vec<_>>(),
        }),
        |out| {
            for l in &landing.landed {
                out.line(format!(
                    "landed {} as trunk@{} ({})",
                    out.id(&l.change_id.to_string()),
                    l.seq.get(),
                    crate::output::short(&l.hash)
                ))?;
            }
            for u in &landing.unlanded {
                out.line(format!(
                    "did not land {}: {}",
                    out.id(&u.change_id.to_string()),
                    u.message
                ))?;
            }
            Ok(())
        },
    )?;
    match landing.unlanded.is_empty() {
        true => Ok(()),
        false => Err(CliError::conflict(format!(
            "{} of the stack's changes did not land",
            landing.unlanded.len()
        ))),
    }
}
