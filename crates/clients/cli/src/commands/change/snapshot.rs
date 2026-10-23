//! `rill new`, `rill describe` and `rill amend`: recording the checkout
//! as a change version, through the daemon.

use rill_daemon::proto as dpb;
use rill_mcp::json;
use rill_sdk::VersionResult;
use serde_json::Value;

use super::args::{AmendArgs, CommitArgs, DescribeArgs, NewArgs};
use super::render::print_version;
use crate::context::{Ctx, TOOL_NAME};
use crate::error::CliError;
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

/// `message`, else one written in the editor on a terminal, starting
/// from `current` and listing the local changes.
async fn message_or_edit(
    ctx: &mut Ctx<'_>,
    message: Option<String>,
    current: &str,
) -> Result<String> {
    if let Some(m) = message {
        return Ok(m);
    }
    if !ctx.interactive() || !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Err(CliError::usage("no message: pass -m <message>"));
    }
    let mount = ctx.mount().await?;
    let changes = ctx
        .daemon()
        .await?
        .get_status(dpb::GetStatusRequest {
            name: mount.name,
            prefixes: vec![],
        })
        .await?
        .changes;
    let mut template = format!("{current}\n\n# Describe the change. Lines starting with '#' are ignored;\n# an empty message aborts.\n#\n# Changes:\n");
    for c in rill_daemon::client::local_changes(&changes) {
        template.push_str(&format!(
            "#   {:<9} {}\n",
            format!("{:?}", c.kind).to_lowercase(),
            c.path
        ));
    }
    crate::output::editor::edit_message(&template)
}

/// Run `rill new`.
pub async fn new(ctx: &mut Ctx<'_>, args: NewArgs) -> Result<()> {
    let message = message_or_edit(ctx, args.message, "").await?;
    let spinner = ctx.spinner("recording the change");
    let committed = commit(ctx, Some(message), args.intent, true).await;
    spinner.finish();
    let (result, stacked_on) = committed?;
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

/// Run `rill commit`: record the working copy as the next version of the
/// working change, starting one if there is none.
pub async fn commit_cmd(ctx: &mut Ctx<'_>, args: CommitArgs) -> Result<()> {
    if ctx.working_change().await?.is_none() {
        return new(
            ctx,
            NewArgs {
                message: args.message,
                intent: args.intent,
            },
        )
        .await;
    }
    amend(
        ctx,
        AmendArgs {
            message: args.message,
        },
    )
    .await
}

/// Run `rill describe`.
pub async fn describe(ctx: &mut Ctx<'_>, args: DescribeArgs) -> Result<()> {
    let current = match ctx.working_change().await? {
        Some(id) => ctx.repo()?.current_version(id).await?.message,
        None => String::new(),
    };
    let message = message_or_edit(ctx, args.message, &current).await?;
    let (result, _) = commit(ctx, Some(message), None, false).await?;
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
