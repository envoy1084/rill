//! Admin tools: identity.

use serde_json::Value;

use super::args::Args;
use super::schema::Props;
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;
use crate::json;

/// Tool definitions in this group.
pub fn definitions() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "rill_whoami",
        description:
            "The principal this server acts as, its teams and the capabilities its token grants.",
        input_schema: Props::default().build(),
    }]
}

/// Dispatch a call in this group.
pub async fn call(ctx: &ToolContext, name: &str, _args: &Args) -> Option<ToolResult<Value>> {
    match name {
        "rill_whoami" => Some(whoami(ctx).await),
        _ => None,
    }
}

async fn whoami(ctx: &ToolContext) -> ToolResult<Value> {
    Ok(json::whoami(&ctx.client().admin().whoami().await?))
}
