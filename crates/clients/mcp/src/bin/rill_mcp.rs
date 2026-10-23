//! `rill-mcp`: the MCP server binary. Speaks MCP on stdin/stdout, or
//! streamable HTTP with `--http`; logs go to stderr so they never corrupt
//! the stdio protocol stream.

use clap::Parser;
use rill_mcp::McpConfig;

/// MCP server exposing a Rill repository to agents over stdio or HTTP.
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

    /// Serve streamable HTTP at this address (path /mcp) instead of stdio.
    #[arg(long, value_name = "ADDR")]
    http: Option<std::net::SocketAddr>,

    /// Extra host names accepted in the Host header over HTTP (loopback
    /// names always are).
    #[arg(long = "allow-host", value_name = "HOST")]
    allow_hosts: Vec<String>,

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
    let served = match flags.http {
        Some(addr) => match config.connect_lazy() {
            Ok((client, repo)) => {
                let ctx = rill_mcp::ToolContext::new(client, repo);
                rill_mcp::http::run_http(ctx, addr, flags.allow_hosts).await
            }
            Err(e) => Err(e),
        },
        None => rill_mcp::run_stdio(config).await,
    };
    match served {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rill-mcp: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
