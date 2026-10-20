//! Working-change commands on a checkout: new, describe, amend, sync,
//! resolve, undo.
//!
//! A checkout tracks one *working change*. `new` snapshots local edits
//! into a change (stacking on top of the previous working change when
//! there is one), `amend` and `describe` re-snapshot into it, `sync` moves
//! the base and merges local edits, `resolve` clears conflict flags after
//! the user edits the markers, and `undo` reverts the server workspace's
//! last operation and restores its base locally.

use clap::Args;
use rill_mcp::json;
use rill_sdk::local::UpdateReport;
use rill_sdk::{ApplyOptions, Checkout, LocalChangeKind, Root, VersionResult};
use rill_types::{IntentId, RepoPathBuf};
use serde_json::{json as j, Value};

use crate::checkout_meta::set_working_change;
use crate::commands::root_arg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{short, Out};
use crate::Result;

/// `rill new`.
#[derive(Args, Debug)]
pub struct NewArgs {
    /// Change message.
    #[arg(short, long)]
    pub message: String,
    /// Intent the change belongs to.
    #[arg(long)]
    pub intent: Option<String>,
}

/// `rill describe`.
#[derive(Args, Debug)]
pub struct DescribeArgs {
    /// New change message.
    #[arg(short, long)]
    pub message: String,
}

/// `rill amend`.
#[derive(Args, Debug)]
pub struct AmendArgs {
    /// Replace the message too (default: keep it).
    #[arg(short, long)]
    pub message: Option<String>,
}

/// `rill sync`.
#[derive(Args, Debug)]
pub struct SyncArgs {
    /// Root to move the base to (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub onto: Option<String>,
}

/// `rill resolve`.
#[derive(Args, Debug)]
pub struct ResolveArgs {
    /// List conflicted paths instead of marking.
    #[arg(long)]
    pub list: bool,
    /// Mark the paths resolved (the default action).
    #[arg(long)]
    pub mark: bool,
    /// Mark every conflicted path resolved.
    #[arg(long)]
    pub all: bool,
    /// Paths to mark resolved.
    pub paths: Vec<String>,
}

fn apply_options(ctx: &Ctx<'_>, message: String, intent: Option<&str>) -> Result<ApplyOptions> {
    let mut opts = ApplyOptions::message(message).with_provenance(ctx.provenance());
    if let Some(i) = intent {
        let id: IntentId = i
            .parse()
            .map_err(|e| CliError::usage(format!("invalid intent id {i:?}: {e}")))?;
        opts = opts.with_intent(id);
    }
    Ok(opts)
}

fn print_version(out: &mut Out<'_>, verb: &str, r: &VersionResult) -> std::io::Result<()> {
    out.line(format!(
        "{verb} change {} version {} ({} file change{})",
        out.id(&r.version.change_id.to_string()),
        out.id(&short(&r.hash)),
        r.version.file_changes.len(),
        if r.version.file_changes.len() == 1 {
            ""
        } else {
            "s"
        },
    ))?;
    let conflicts = r.conflicted_paths();
    if !conflicts.is_empty() {
        out.line(out.bad(&format!(
            "conflicts in: {}",
            conflicts
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )))?;
    }
    Ok(())
}

/// Run `rill new`.
pub async fn new(ctx: &mut Ctx<'_>, args: NewArgs) -> Result<()> {
    let dir = ctx.require_checkout_dir()?;
    let mut checkout = ctx.checkout().await?;
    let mut stacked_on = None;
    if let Some(previous) = checkout.meta().working_change {
        let current = checkout.repo().current_version(previous).await?;
        if current.trunk_seq.is_some() {
            // The previous working change landed; the checkout simply
            // starts a fresh change on its current base.
            set_working_change(&dir, None).await?;
            checkout = ctx.checkout().await?;
            return snapshot_new(ctx, checkout, args, None).await;
        }
        // Stack: the previous working change becomes the base, so only
        // edits made since the last snapshot go into the new change.
        let report = checkout.update(Root::Version(current.hash)).await?;
        if !report.conflicts.is_empty() {
            return Err(CliError::conflict(format!(
                "cannot start a new change: moving onto {} conflicts in {}",
                short(&current.hash),
                join_paths(&report.conflicts)
            )));
        }
        set_working_change(&dir, None).await?;
        checkout = ctx.checkout().await?;
        stacked_on = Some(previous);
    }
    snapshot_new(ctx, checkout, args, stacked_on).await
}

async fn snapshot_new(
    ctx: &mut Ctx<'_>,
    mut checkout: Checkout,
    args: NewArgs,
    stacked_on: Option<rill_types::ChangeId>,
) -> Result<()> {
    let opts = apply_options(ctx, args.message, args.intent.as_deref())?;
    let result = checkout.snapshot(opts).await?;
    // The version records the intent; the intent's change list is a
    // separate record that only the link RPC updates.
    if let Some(intent) = result.version.intent {
        checkout
            .repo()
            .intents()
            .link_change(intent, result.version.change_id)
            .await?;
    }
    let mut value = json::version_result(&result);
    value["stacked_on"] = stacked_on
        .map(|c| Value::String(c.to_string()))
        .unwrap_or(Value::Null);
    ctx.emit(value, |out| {
        print_version(out, "created", &result)?;
        if let Some(prev) = stacked_on {
            out.line(out.dim(&format!("stacked on {prev}")))?;
        }
        Ok(())
    })
}

fn require_working_change(checkout: &Checkout) -> Result<rill_types::ChangeId> {
    checkout
        .meta()
        .working_change
        .ok_or_else(|| CliError::usage("no working change yet: run `rill new -m <message>` first"))
}

/// Run `rill describe`.
pub async fn describe(ctx: &mut Ctx<'_>, args: DescribeArgs) -> Result<()> {
    let mut checkout = ctx.checkout().await?;
    require_working_change(&checkout)?;
    // The SDK has no message-only amend; a snapshot with the new message
    // re-uploads the (unchanged) edits and produces the described version.
    let opts = apply_options(ctx, args.message, None)?;
    let result = checkout.snapshot(opts).await?;
    ctx.emit(json::version_result(&result), |out| {
        print_version(out, "described", &result)
    })
}

/// Run `rill amend`.
pub async fn amend(ctx: &mut Ctx<'_>, args: AmendArgs) -> Result<()> {
    let mut checkout = ctx.checkout().await?;
    let id = require_working_change(&checkout)?;
    let message = match args.message {
        Some(m) => m,
        None => checkout.repo().current_version(id).await?.message,
    };
    let opts = apply_options(ctx, message, None)?;
    let result = checkout.snapshot(opts).await?;
    ctx.emit(json::version_result(&result), |out| {
        print_version(out, "amended", &result)
    })
}

fn join_paths(paths: &[RepoPathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn print_report(out: &mut Out<'_>, report: &UpdateReport) -> std::io::Result<()> {
    for p in &report.updated {
        out.line(format!("{} {p}", out.good("U")))?;
    }
    for p in &report.removed {
        out.line(format!("{} {p}", out.bad("D")))?;
    }
    for p in &report.merged {
        out.line(format!("{} {p}", out.warn("M")))?;
    }
    for p in &report.conflicts {
        out.line(format!("{} {p}", out.bad("C")))?;
    }
    Ok(())
}

/// Run `rill sync`.
pub async fn sync(ctx: &mut Ctx<'_>, args: SyncArgs) -> Result<()> {
    let onto = root_arg(args.onto.as_deref())?;
    let mut checkout = ctx.checkout().await?;
    if let Some(ws) = checkout.meta().workspace {
        // Keep the server-side workspace's base in step; a failure here is
        // not fatal for the local update, so it is logged rather than raised.
        if let Err(e) = checkout
            .repo()
            .workspaces()
            .update_base(ws, Some(onto.clone()))
            .await
        {
            tracing::warn!(workspace = %ws, error = %e, "workspace base not updated");
        }
    }
    let report = checkout.update(onto.clone()).await?;
    forget_landed_working_change(ctx, &checkout).await?;
    let base_version = checkout.meta().base_version;
    let conflicts = report.conflicts.clone();
    let mut value = json::update_report(&report);
    value["base"] = Value::String(onto.to_string());
    value["base_version"] = base_version
        .map(|h| Value::String(h.to_hex()))
        .unwrap_or(Value::Null);
    ctx.emit(value, |out| {
        out.line(format!(
            "synced to {onto}{}",
            base_version
                .map(|h| format!(" ({})", short(&h)))
                .unwrap_or_default()
        ))?;
        print_report(out, &report)?;
        if report.updated.is_empty()
            && report.removed.is_empty()
            && report.merged.is_empty()
            && report.conflicts.is_empty()
        {
            out.line(out.dim("already up to date"))?;
        }
        Ok(())
    })?;
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(CliError::conflict(format!(
            "{} path(s) have conflict markers: {}; edit them, then `rill resolve <paths>`",
            conflicts.len(),
            join_paths(&conflicts)
        )))
    }
}

/// Clear the checkout's working change once it has landed, so the next
/// `new` starts fresh instead of stacking on history.
async fn forget_landed_working_change(ctx: &mut Ctx<'_>, checkout: &Checkout) -> Result<()> {
    let Some(id) = checkout.meta().working_change else {
        return Ok(());
    };
    let landed = match checkout.repo().current_version(id).await {
        Ok(info) => info.trunk_seq.is_some(),
        Err(e) if e.is_not_found() => true,
        Err(e) => return Err(e.into()),
    };
    if landed {
        let dir = ctx.require_checkout_dir()?;
        set_working_change(&dir, None).await?;
    }
    Ok(())
}

/// Run `rill resolve`.
pub async fn resolve(ctx: &mut Ctx<'_>, args: ResolveArgs) -> Result<()> {
    let mut checkout = ctx.checkout().await?;
    let conflicted: Vec<RepoPathBuf> = checkout
        .status()
        .await?
        .into_iter()
        .filter(|c| c.kind == LocalChangeKind::Conflict)
        .map(|c| c.path)
        .collect();
    // With --list, or with nothing to mark, show what is conflicted and how
    // to mark it rather than silently doing nothing.
    if args.list || (args.paths.is_empty() && !args.all) {
        return ctx.emit(
            j!({ "conflicts": conflicted.iter().map(|p| p.as_str()).collect::<Vec<_>>() }),
            |out| {
                if conflicted.is_empty() {
                    return out.line(out.dim("no conflicted paths"));
                }
                for p in &conflicted {
                    out.line(format!("{} {p}", out.bad("C")))?;
                }
                out.line(
                    out.dim(
                        "edit the markers, then `rill resolve <paths>` or `rill resolve --all`",
                    ),
                )
            },
        );
    }
    let paths: Vec<RepoPathBuf> = if args.all {
        conflicted.clone()
    } else {
        args.paths
            .iter()
            .map(RepoPathBuf::new)
            .collect::<std::result::Result<_, _>>()?
    };
    for p in &paths {
        if !conflicted.contains(p) {
            return Err(CliError::usage(format!("{p} is not conflicted")));
        }
    }
    checkout.mark_resolved(&paths).await?;
    let remaining = conflicted.len() - paths.len();
    ctx.emit(
        j!({
            "resolved": paths.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            "remaining": remaining,
        }),
        |out| {
            out.line(format!("marked {} path(s) resolved", paths.len()))?;
            if remaining > 0 {
                out.line(out.warn(&format!("{remaining} still conflicted")))?;
            }
            Ok(())
        },
    )
}

/// Run `rill undo`.
pub async fn undo(ctx: &mut Ctx<'_>) -> Result<()> {
    let mut checkout = ctx.checkout().await?;
    let ws = checkout.meta().workspace.ok_or_else(|| {
        CliError::usage(
            "this checkout has no server workspace to undo (created with --no-workspace)",
        )
    })?;
    let op = checkout.repo().workspaces().undo(ws, None).await?;
    let base = op.workspace.base;
    let report = checkout.update(Root::Version(base)).await?;
    let mut value = json::workspace_op(&op);
    value["update"] = json::update_report(&report);
    ctx.emit(value, |out| {
        out.line(format!(
            "undid operation {}; base is now {}",
            out.id(&op.operation.id.to_string()),
            out.id(&short(&base))
        ))?;
        print_report(out, &report)
    })?;
    if report.conflicts.is_empty() {
        Ok(())
    } else {
        Err(CliError::conflict(format!(
            "conflicts in {}",
            join_paths(&report.conflicts)
        )))
    }
}
