//! The MCP server: method dispatch and the stdio session loop.
//!
//! Supported methods: `initialize`, `notifications/initialized`, `ping`,
//! `tools/list` and `tools/call`. Tool failures become results with
//! `isError: true`; only protocol-level problems (bad JSON, unknown
//! method, unknown tool, malformed params) become JSON-RPC errors. Nothing
//! an agent sends can stop the loop; only transport errors do.

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWrite};

use crate::error::{Result, ToolError};
use crate::protocol::{
    read_message, write_message, Request, Response, RpcError, INVALID_PARAMS, INVALID_REQUEST,
    METHOD_NOT_FOUND, PARSE_ERROR, PROTOCOL_VERSION,
};
use crate::tools::{self, ToolContext};

/// An MCP server bound to one Rill connection.
#[derive(Clone, Debug)]
pub struct McpServer {
    ctx: ToolContext,
}

impl McpServer {
    /// Build a server over a tool context.
    pub fn new(ctx: ToolContext) -> Self {
        McpServer { ctx }
    }

    /// Handle one decoded message. `None` means nothing is sent back (a
    /// notification).
    pub async fn handle_value(&self, message: Value) -> Option<Response> {
        let request: Request = match serde_json::from_value(message) {
            Ok(r) => r,
            Err(e) => {
                return Some(Response::failure(
                    None,
                    RpcError::new(INVALID_REQUEST, format!("invalid request: {e}")),
                ))
            }
        };
        self.handle(request).await
    }

    /// Handle one request or notification.
    pub async fn handle(&self, request: Request) -> Option<Response> {
        let id = request.id.clone();
        let outcome = match request.method.as_str() {
            "initialize" => Ok(self.initialize()),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.list_tools()),
            "tools/call" => self.call_tool(&request.params).await,
            m if m.starts_with("notifications/") => return None,
            other => Err(RpcError::new(
                METHOD_NOT_FOUND,
                format!("method not found: {other}"),
            )),
        };
        if request.is_notification() {
            // A notification for a request method still gets no reply.
            return None;
        }
        Some(match outcome {
            Ok(result) => Response::success(id, result),
            Err(error) => Response::failure(id, error),
        })
    }

    fn initialize(&self) -> Value {
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": {
                "name": "rill-mcp",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": "Tools expose a Rill source-control server. Pass `repo` to address a repository unless RILL_REPO is set. Roots are `trunk`, `trunk@<seq>`, a version hash or a ref name. Writes record provenance; pass `model` and `session` so changes are attributable.",
        })
    }

    fn list_tools(&self) -> Value {
        json!({ "tools": tools::definitions().iter().map(|d| d.to_json()).collect::<Vec<_>>() })
    }

    async fn call_tool(&self, params: &Value) -> std::result::Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, "tools/call requires a `name`"))?;
        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
        tracing::debug!(tool = name, "tool call");
        match tools::call(&self.ctx, name, arguments).await {
            None => Err(RpcError::new(
                INVALID_PARAMS,
                format!("unknown tool: {name}"),
            )),
            Some(Ok(value)) => Ok(tool_result(value)),
            Some(Err(e)) => {
                tracing::debug!(tool = name, error = %e, "tool failed");
                Ok(tool_error(&e))
            }
        }
    }

    /// Run the session: read newline-delimited JSON-RPC from `reader`,
    /// write responses to `writer`, until end of input.
    pub async fn serve<R, W>(&self, mut reader: R, mut writer: W) -> Result<()>
    where
        R: AsyncBufReadExt + Unpin,
        W: AsyncWrite + Unpin,
    {
        while let Some(message) = read_message(&mut reader).await? {
            let response = match message {
                Ok(Value::Array(_)) => Some(Response::failure(
                    None,
                    RpcError::new(INVALID_REQUEST, "batch requests are not supported"),
                )),
                Ok(value) => self.handle_value(value).await,
                Err(text) => Some(Response::failure(
                    None,
                    RpcError::new(PARSE_ERROR, format!("parse error: {text}")),
                )),
            };
            if let Some(response) = response {
                let value = serde_json::to_value(&response)?;
                write_message(&mut writer, &value).await?;
            }
        }
        Ok(())
    }
}

/// A successful tool result: the JSON as text content plus structured
/// content for clients that read it.
fn tool_result(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": value,
        "isError": false,
    })
}

/// A failed tool result.
fn tool_error(e: &ToolError) -> Value {
    let structured = json!({ "error": { "code": e.code(), "message": e.to_string() } });
    json!({
        "content": [{ "type": "text", "text": format!("error ({}): {e}", e.code()) }],
        "structuredContent": structured,
        "isError": true,
    })
}

impl From<serde_json::Error> for crate::error::Error {
    fn from(e: serde_json::Error) -> Self {
        crate::error::Error::Io(e.into())
    }
}
