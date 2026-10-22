//! `rill new`, `rill describe` and `rill amend`: snapshotting local edits.

use rill_mcp::json;
use rill_sdk::{ApplyOptions, Checkout, Root};
use rill_types::IntentId;
use serde_json::Value;

use super::args::{AmendArgs, DescribeArgs, NewArgs};
use super::render::{join_paths, print_version};
use crate::checkout_meta::set_working_change;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::short;
use crate::Result;

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
