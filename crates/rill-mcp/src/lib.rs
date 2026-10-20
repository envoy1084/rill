//! MCP server for Rill.
//!
//! `rill-mcp` speaks the Model Context Protocol over stdio (newline-
//! delimited JSON-RPC 2.0) and exposes the SDK as tools so agents can read
//! history, edit, review and land without a checkout. The protocol is
//! implemented by hand in [`protocol`] and [`server`]; it is small enough
//! (`initialize`, `tools/list`, `tools/call`, `ping`) that a dependency
//! would cost more than it saves.
//!
//! [`json`] is the machine-readable rendering of SDK values. The CLI's
//! `--json` output uses the same functions so agents see one schema
//! whichever surface they use.

#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod grep;
pub mod json;
pub mod protocol;
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
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    server.serve(stdin, stdout).await
}
