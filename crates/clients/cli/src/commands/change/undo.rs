//! `rill undo`: reverting the server workspace's last operation.

use rill_mcp::json;
use rill_sdk::Root;

use super::render::{join_paths, print_report};
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::short;
use crate::Result;

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
