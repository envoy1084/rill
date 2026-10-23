//! `rill sync`: moving the checkout onto a new base.

use rill_daemon::proto as dpb;
use serde_json::Value;

use super::args::SyncArgs;
use super::render::{join_paths, print_report, update_json};
use crate::context::Ctx;
use crate::error::CliError;
use crate::Result;

/// Run `rill sync`.
pub async fn sync(ctx: &mut Ctx<'_>, args: SyncArgs) -> Result<()> {
    let onto = crate::commands::root_arg(args.onto.as_deref())?;
    let report = update_checkout(ctx, &onto.to_string()).await?;
    let mount = report.mount.clone().into_option().unwrap_or_default();
    let mut value = update_json(&report);
    value["base"] = Value::String(onto.to_string());
    value["base_version"] = if mount.base_version.is_empty() {
        Value::Null
    } else {
        Value::String(mount.base_version.clone())
    };
    ctx.emit(value, |out| {
        out.line(format!(
            "synced to {onto}{}",
            mount
                .base_version
                .get(..12)
                .map(|h| format!(" ({h})"))
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
    if report.conflicts.is_empty() {
        Ok(())
    } else {
        Err(CliError::conflict(format!(
            "{} path(s) have conflict markers: {}; edit them, then `rill resolve <paths>`",
            report.conflicts.len(),
            join_paths(&report.conflicts)
        )))
    }
}

/// Move the checkout and its server workspace onto `revision`, merging
/// local edits; a landed working change is forgotten.
pub(crate) async fn update_checkout(
    ctx: &mut Ctx<'_>,
    revision: &str,
) -> Result<dpb::UpdateMountResponse> {
    let mount = ctx.mount().await?;
    let daemon = ctx.daemon().await?;
    let spinner = ctx.spinner(format!("moving onto {revision}"));
    let report = daemon
        .update_mount(dpb::UpdateMountRequest {
            name: mount.name,
            revision: revision.to_string(),
            ..Default::default()
        })
        .await;
    spinner.finish();
    Ok(report?)
}
