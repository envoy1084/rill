//! `rill sync`: moving the checkout onto a new base.

use rill_mcp::json;
use rill_sdk::local::UpdateReport;
use rill_sdk::{Checkout, Root};
use serde_json::Value;

use super::args::SyncArgs;
use super::render::{join_paths, print_report};
use crate::checkout_meta::set_working_change;
use crate::commands::root_arg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::short;
use crate::Result;

/// Run `rill sync`.
pub async fn sync(ctx: &mut Ctx<'_>, args: SyncArgs) -> Result<()> {
    let onto = root_arg(args.onto.as_deref())?;
    let mut checkout = ctx.checkout().await?;
    let report = update_checkout(ctx, &mut checkout, &onto).await?;
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

/// Move the checkout and its server workspace to `onto`, merging local
/// edits, and forget the working change if it has landed.
pub(crate) async fn update_checkout(
    ctx: &mut Ctx<'_>,
    checkout: &mut Checkout,
    onto: &Root,
) -> Result<UpdateReport> {
    if let Some(ws) = checkout.meta().workspace {
        // The local update must not fail because the advisory server-side
        // workspace could not follow, so this is logged rather than raised.
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
    forget_landed_working_change(ctx, checkout).await?;
    Ok(report)
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
