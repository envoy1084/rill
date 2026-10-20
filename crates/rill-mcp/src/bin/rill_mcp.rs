//! `rill-mcp`: the MCP server binary. Speaks MCP on stdin/stdout; logs go
//! to stderr so they never corrupt the protocol stream.

use clap::Parser;
use rill_mcp::McpConfig;

/// MCP server exposing a Rill repository to agents over stdio.
#[derive(Parser, Debug)]
#[command(name = "rill-mcp", version, about)]
struct Flags {
    /// Server endpoint, e.g. http://localhost:7420.
    #[arg(long, env = "RILL_ENDPOINT")]
    endpoint: Option<String>,

    /// Bearer token.
    #[arg(long, env = "RILL_TOKEN", hide_env_values = true)]
    token: Option<String>,

    /// Repository used when a tool call names none.
    #[arg(long, env = "RILL_REPO")]
    repo: Option<String>,

    /// Print the tool list as JSON and exit.
    #[arg(long)]
    list_tools: bool,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let flags = Flags::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    if flags.list_tools {
        let tools: Vec<_> = rill_mcp::tools::definitions()
            .iter()
            .map(|d| d.to_json())
            .collect();
        let text = serde_json::to_string_pretty(&tools).unwrap_or_default();
        println!("{text}");
        return std::process::ExitCode::SUCCESS;
    }

    let config = McpConfig {
        endpoint: flags.endpoint,
        token: flags.token,
        repo: flags.repo,
    };
    match rill_mcp::run_stdio(config).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rill-mcp: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
