//! The MCP server: rmcp speaks the protocol (initialize, version
//! negotiation, `tools/list`, `tools/call`, ping, cancellation) over any
//! transport; this module advertises Rill's tools and dispatches calls.

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{json, Value};

use crate::error::ToolError;
use crate::tools::{self, ToolContext};

const INSTRUCTIONS: &str = "Tools expose a Rill source-control server. Pass `repo` to address a repository unless RILL_REPO is set. Roots are `trunk`, `trunk@<seq>`, a version hash or a ref name. Writes record provenance; pass `model` and `session` so changes are attributable.";

/// Serves Rill's tools to one MCP client.
#[derive(Clone, Debug)]
pub struct McpServer {
    ctx: ToolContext,
}

impl McpServer {
    /// A server answering with `ctx`'s connection.
    pub fn new(ctx: ToolContext) -> Self {
        McpServer { ctx }
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = rmcp::model::Implementation::default();
        info.name = "rill-mcp".into();
        info.version = env!("CARGO_PKG_VERSION").into();
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(info)
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = tools::definitions()
            .into_iter()
            .map(|d| {
                let schema = match d.input_schema {
                    Value::Object(map) => map,
                    _ => serde_json::Map::new(),
                };
                Tool::new(d.name, d.description, Arc::new(schema))
            })
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.as_ref();
        let arguments = request.arguments.map(Value::Object).unwrap_or(Value::Null);
        tracing::debug!(tool = name, "tool call");
        match tools::call(&self.ctx, name, arguments).await {
            None => Err(ErrorData::invalid_params(
                format!("unknown tool: {name}"),
                None,
            )),
            Some(Ok(value)) => Ok(tool_result(value).into()),
            Some(Err(e)) => {
                tracing::debug!(tool = name, error = %e, "tool failed");
                Ok(tool_error(&e).into())
            }
        }
    }
}

/// A successful tool result: the JSON as text content plus structured
/// content for clients that read it.
fn tool_result(value: Value) -> CallToolResult {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(value);
    result
}

/// A failed tool result, with the error's code and, when the server
/// refused, its reason, what is missing and how long to wait.
fn tool_error(e: &ToolError) -> CallToolResult {
    let (reason, missing, retry_after) = match e {
        ToolError::Sdk(e) => (
            e.reason().map(rill_sdk::error::reason_name),
            e.missing().to_vec(),
            e.retry_after(),
        ),
        _ => (None, Vec::new(), None),
    };
    let mut text = format!("error ({}): {e}", reason.unwrap_or(&e.code()));
    for m in &missing {
        text.push_str(&format!("\nneeds: {m}"));
    }
    let mut result = CallToolResult::error(vec![ContentBlock::text(text)]);
    result.structured_content = Some(json!({ "error": {
        "code": e.code(),
        "reason": reason,
        "message": e.to_string(),
        "missing": missing,
        "retry_after_ms": retry_after.map(|d| d.as_millis() as u64),
    } }));
    result
}
