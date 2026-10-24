//! MCP server for Rill.
//!
//! `rill-mcp` exposes Rill to agents over the Model Context Protocol, on
//! stdio or streamable HTTP. The protocol is the official Rust SDK,
//! `rmcp`. [`tools`] holds fourteen task-level tools (read, search, edit,
//! check, commit, review, land) that remember the session's change, fit
//! their output to a token budget and name the next step; [`resources`]
//! makes files, diffs and reviews addressable and subscribable.
//!
//! [`json`] is the machine-readable rendering of SDK values. The CLI's
//! `--json` output uses the same functions so agents see one schema
//! whichever surface they use.

#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod http;
pub mod json;
pub mod pool;
pub mod resources;
pub mod server;
pub mod session;
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
