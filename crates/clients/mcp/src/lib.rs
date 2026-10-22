//! MCP server for Rill.
//!
//! `rill-mcp` exposes the SDK as Model Context Protocol tools so agents can
//! read history, edit, review and land without a checkout. The protocol is
//! the official Rust SDK, `rmcp`; [`server`] advertises the tools and
//! dispatches calls.
//!
//! [`json`] is the machine-readable rendering of SDK values. The CLI's
//! `--json` output uses the same functions so agents see one schema
//! whichever surface they use.

#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod grep;
pub mod json;
pub mod server;
pub mod tools;

pub use config::McpConfig;
pub use error::{Error, Result, ToolError, ToolResult};
pub use server::McpServer;
pub use tools::{ToolContext, ToolDef};

/// Run the stdio session for `config` until stdin closes.
pub async fn run_stdio(config: McpConfig) -> Result<()> {
    let (client, repo) = config.connect_lazy()?;
    let server = McpServer::new(ToolContext::new(client, repo));
    serve(server, rmcp::transport::stdio()).await
}

/// Serve `server` over `transport` (stdio, or a pipe in tests) until the
/// client goes away.
pub async fn serve<T, E, A>(server: McpServer, transport: T) -> Result<()>
where
    T: rmcp::transport::IntoTransport<rmcp::RoleServer, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    use rmcp::ServiceExt;
    let running = server
        .serve(transport)
        .await
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    running
        .waiting()
        .await
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    Ok(())
}
