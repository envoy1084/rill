//! `rill undo`: reverting the server workspace's last operation.

use rill_mcp::json;

use super::render::{join_paths, print_report, update_json};
use super::sync::update_checkout;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::short;
use crate::Result;

/// Run `rill undo`.
pub async fn undo(ctx: &mut Ctx<'_>) -> Result<()> {
    let ws = ctx.checkout_workspace().await?.ok_or_else(|| {
        CliError::usage(
            "this checkout has no server workspace to undo (created with --no-workspace)",
        )
    })?;
    let op = ctx.repo()?.workspaces().undo(ws, None).await?;
    let base = op.workspace.base;
    // A workspace made on an empty repository has no base version: its
    // base is the empty tree.
    let revision = if base.is_zero() {
        rill_sdk::Root::Tree(rill_types::Tree::empty_hash()).to_string()
    } else {
        base.to_hex()
    };
    let report = update_checkout(ctx, &revision).await?;
    let mut value = json::workspace_op(&op);
    value["update"] = update_json(&report);
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

/// Run `rill redo`: undo the undo that was the workspace's last operation.
pub async fn redo(ctx: &mut Ctx<'_>) -> Result<()> {
    let ws = ctx.checkout_workspace().await?.ok_or_else(|| {
        CliError::usage("this checkout has no server workspace (created with --no-workspace)")
    })?;
    let repo = ctx.repo()?;
    let head = repo.workspaces().operations(ws, 1, None).await?.items;
    if !matches!(
        head.first().map(|op| &op.kind),
        Some(rill_types::OperationKind::Undo { .. })
    ) {
        return Err(CliError::usage(
            "nothing to redo: the last operation was not an undo",
        ));
    }
    undo(ctx).await
}
