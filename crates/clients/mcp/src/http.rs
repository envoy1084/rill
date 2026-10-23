//! The streamable HTTP transport: MCP at `/mcp` on an axum server. Each
//! session gets its own [`Session`](crate::session::Session); callers may
//! send their own `Authorization: Bearer` token, used instead of the
//! configured one.

use std::net::SocketAddr;
use std::sync::Arc;

use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::RoleServer;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::server::McpServer;
use crate::tools::ToolContext;

/// Where the MCP endpoint is mounted.
pub const PATH: &str = "/mcp";

/// Serve `ctx` over streamable HTTP on `listener` until `shutdown`.
/// `allowed_hosts` extends the loopback names accepted in `Host`, which
/// guards local servers against DNS rebinding.
pub async fn serve_http(
    ctx: ToolContext,
    listener: tokio::net::TcpListener,
    allowed_hosts: Vec<String>,
    shutdown: CancellationToken,
) -> Result<()> {
    let mut config =
        StreamableHttpServerConfig::default().with_cancellation_token(shutdown.clone());
    if !allowed_hosts.is_empty() {
        let mut hosts = config.allowed_hosts.clone();
        hosts.extend(allowed_hosts);
        config = config.with_allowed_hosts(hosts);
    }
    let service = StreamableHttpService::new(
        move || Ok(McpServer::new(ctx.new_session())),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let router = axum::Router::new().nest_service(PATH, service);
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { shutdown.cancelled().await })
        .await
        .map_err(Error::Io)
}

/// Bind `addr` and serve until interrupted.
pub async fn run_http(
    ctx: ToolContext,
    addr: SocketAddr,
    allowed_hosts: Vec<String>,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(addr = %listener.local_addr()?, path = PATH, "serving MCP over HTTP");
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            stop.cancel();
        }
    });
    serve_http(ctx, listener, allowed_hosts, shutdown).await
}

/// The bearer token of the HTTP request behind `rc`; `None` over stdio.
pub fn bearer(rc: &RequestContext<RoleServer>) -> Option<String> {
    let parts = rc.extensions.get::<http::request::Parts>()?;
    let value = parts
        .headers
        .get(http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?;
    Some(token.trim().to_string())
}
