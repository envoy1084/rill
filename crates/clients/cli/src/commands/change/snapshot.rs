//! `rill new`, `rill describe` and `rill amend`: recording the checkout
//! as a change version, through the daemon.

use rill_daemon::proto as dpb;
use rill_mcp::json;
use rill_sdk::VersionResult;
use serde_json::Value;

use super::args::{AmendArgs, DescribeArgs, NewArgs};
use super::render::print_version;
use crate::context::{Ctx, TOOL_NAME};
use crate::Result;

async fn commit(
    ctx: &mut Ctx<'_>,
    message: Option<String>,
    intent: Option<String>,
    new_change: bool,
) -> Result<(VersionResult, Option<String>)> {
    let mount = ctx.mount().await?;
    let done = ctx
        .daemon()
        .await?
        .commit_mount(dpb::CommitMountRequest {
            name: mount.name,
            keep_message: message.is_none(),
            message: message.unwrap_or_default(),
            intent: intent.unwrap_or_default(),
            new_change,
            tool: TOOL_NAME.into(),
            ..Default::default()
        })
        .await?;
    let hash = done.version.parse()?;
    let version = ctx.repo()?.change_version(hash).await?;
    let stacked_on = (!done.stacked_on.is_empty()).then_some(done.stacked_on);
    Ok((VersionResult { hash, version }, stacked_on))
}

/// Run `rill new`.
pub async fn new(ctx: &mut Ctx<'_>, args: NewArgs) -> Result<()> {
    let (result, stacked_on) = commit(ctx, Some(args.message), args.intent, true).await?;
    let mut value = json::version_result(&result);
    value["stacked_on"] = stacked_on.clone().map(Value::String).unwrap_or(Value::Null);
    ctx.emit(value, |out| {
        print_version(out, "created", &result)?;
        if let Some(prev) = &stacked_on {
            out.line(out.dim(&format!("stacked on {prev}")))?;
        }
        Ok(())
    })
}

/// Run `rill describe`.
pub async fn describe(ctx: &mut Ctx<'_>, args: DescribeArgs) -> Result<()> {
    let (result, _) = commit(ctx, Some(args.message), None, false).await?;
    ctx.emit(json::version_result(&result), |out| {
        print_version(out, "described", &result)
    })
}

/// Run `rill amend`.
pub async fn amend(ctx: &mut Ctx<'_>, args: AmendArgs) -> Result<()> {
    let (result, _) = commit(ctx, args.message, None, false).await?;
    ctx.emit(json::version_result(&result), |out| {
        print_version(out, "amended", &result)
    })
}
