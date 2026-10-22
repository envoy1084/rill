//! Lander tools: trunk status.

use serde_json::Value;

use super::args::Args;
use super::schema::{integer, Props};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;
use crate::json;

/// Tool definitions in this group.
pub fn definitions() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "rill_trunk_status",
        description: "Trunk head sequence and hash, landings in flight, recent landings and the last verified sequence.",
        input_schema: Props::repo()
            .opt("recent_limit", integer("How many recent landings to include (default 10)"))
            .build(),
    }]
}

/// Dispatch a call in this group.
pub async fn call(ctx: &ToolContext, name: &str, args: &Args) -> Option<ToolResult<Value>> {
    match name {
        "rill_trunk_status" => Some(trunk_status(ctx, args).await),
        _ => None,
    }
}

async fn trunk_status(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let lander = ctx.repo(args)?.lander();
    let status = lander.trunk_status(args.limit("recent_limit", 10)?).await?;
    Ok(json::trunk_status(&status))
}
