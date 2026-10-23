//! The MCP server: rmcp speaks the protocol (initialize, version
//! negotiation, ping, cancellation) over any transport; this module
//! advertises Rill's tools and resources and dispatches to them.

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResponse, ResourceUpdatedNotificationParam,
    ServerCapabilities, ServerConfig, SubscribeRequestParams, SubscriptionFilter, Tool,
    UnsubscribeRequestParams,
};
use rmcp::service::{RequestContext, SubscriptionContext};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{json, Value};

use crate::error::ToolError;
use crate::http::bearer;
use crate::resources;
use crate::tools::{self, call_key, stop_hint, ToolContext};

const INSTRUCTIONS: &str = "Rill source control. Call rill_context first. The loop is rill_edit (starts a change, then amends it) -> rill_check -> rill_commit (submits for review) -> rill_review_status / rill_watch / rill_respond -> rill_land. Tools default to the session's change and RILL_REPO; pass `change` and `repo` explicitly over stateless HTTP. Every result names the next step. Pass `model` and `session` to writing tools so changes are attributable.";

/// Serves Rill's tools and resources to one MCP session.
#[derive(Clone, Debug)]
pub struct McpServer {
    ctx: ToolContext,
}

impl McpServer {
    /// A server answering with `ctx`'s connection and session.
    pub fn new(ctx: ToolContext) -> Self {
        McpServer { ctx }
    }

    /// The context for one request: the caller's bearer token (over
    /// HTTP) picks the client.
    fn request_ctx(&self, rc: &RequestContext<RoleServer>) -> Result<ToolContext, ErrorData> {
        self.ctx
            .with_token(bearer(rc).as_deref())
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = rmcp::model::Implementation::default();
        info.name = "rill-mcp".into();
        info.version = env!("CARGO_PKG_VERSION").into();
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .build();
        ServerConfig::new(capabilities)
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
        rc: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let ctx = self.request_ctx(&rc)?;
        let name = request.name.as_ref();
        let arguments = request.arguments.map(Value::Object).unwrap_or(Value::Null);
        let key = call_key(name, &arguments);
        tracing::debug!(tool = name, "tool call");
        match tools::call(&ctx, name, arguments).await {
            None => Err(ErrorData::invalid_params(
                format!("unknown tool: {name}"),
                None,
            )),
            Some(Ok(value)) => {
                ctx.session().guard(|g| g.succeeded());
                Ok(tool_result(value).into())
            }
            Some(Err(e)) => {
                tracing::debug!(tool = name, error = %e, "tool failed");
                let repeats = ctx.session().guard(|g| g.failed(key));
                Ok(tool_error(&e, stop_hint(repeats)).into())
            }
        }
    }

    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        rc: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let ctx = self.request_ctx(&rc)?;
        Ok(ListResourcesResult::with_all_items(resources::listed(&ctx)))
    }

    async fn list_resource_templates(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult::with_all_items(
            resources::templates(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        rc: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let ctx = self.request_ctx(&rc)?;
        match resources::read(&ctx, &request.uri).await {
            Ok(result) => Ok(result.into()),
            Err(e @ ToolError::InvalidArgs(_)) => {
                Err(ErrorData::invalid_params(e.to_string(), None))
            }
            Err(e) => Err(ErrorData::resource_not_found(
                e.to_string(),
                Some(json!({ "uri": request.uri, "code": e.code() })),
            )),
        }
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        let uris = requested.resource_subscriptions.as_deref().unwrap_or(&[]);
        let mut accepted = SubscriptionFilter::new();
        accepted.resource_subscriptions = Some(resources::watchable(uris));
        Some(accepted)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        use futures::StreamExt;
        let ctx = self.request_ctx(context.request_context())?;
        let uris = context
            .accepted()
            .resource_subscriptions
            .clone()
            .unwrap_or_default();
        let mut updates = resources::updates(&ctx, &uris).await;
        loop {
            tokio::select! {
                () = context.cancelled() => return Ok(()),
                next = updates.next() => match next {
                    Some(uris) => for uri in uris {
                        if let Err(e) = context.sink().notify_resource_updated(uri).await {
                            tracing::debug!(error = %e, "subscription closed");
                            return Ok(());
                        }
                    },
                    None => return Ok(()),
                },
            }
        }
    }

    #[allow(deprecated)]
    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        rc: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        use futures::StreamExt;
        let ctx = self.request_ctx(&rc)?;
        let uri = request.uri;
        if resources::watchable(std::slice::from_ref(&uri)).is_empty() {
            return Err(ErrorData::invalid_params(
                format!("{uri} does not send updates; only change diff and review resources do"),
                None,
            ));
        }
        let mut updates = resources::updates(&ctx, std::slice::from_ref(&uri)).await;
        let peer = rc.peer.clone();
        let target = uri.clone();
        let task = tokio::spawn(async move {
            while updates.next().await.is_some() {
                let param = ResourceUpdatedNotificationParam::new(target.clone());
                if peer.notify_resource_updated(param).await.is_err() {
                    return;
                }
            }
        });
        ctx.session().subscribe(uri, task.abort_handle());
        Ok(())
    }

    #[allow(deprecated)]
    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        rc: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        self.request_ctx(&rc)?.session().unsubscribe(&request.uri);
        Ok(())
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

/// A failed tool result: the error's code and, when the server refused,
/// its reason, what is missing and how long to wait; with a stop hint
/// once the same call keeps failing.
fn tool_error(e: &ToolError, stop: Option<String>) -> CallToolResult {
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
    if let Some(stop) = &stop {
        text.push_str(&format!("\nstop: {stop}"));
    }
    let mut result = CallToolResult::error(vec![ContentBlock::text(text)]);
    result.structured_content = Some(json!({ "error": {
        "code": e.code(),
        "reason": reason,
        "message": e.to_string(),
        "missing": missing,
        "retry_after_ms": retry_after.map(|d| d.as_millis() as u64),
        "stop": stop,
    } }));
    result
}
