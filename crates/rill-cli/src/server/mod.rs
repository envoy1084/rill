//! The `rill-server` binary: flags and environment on top of
//! [`rill_server::ServerConfig`], logging, and a ctrl-c driven shutdown.

mod template;

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rill_server::{ServerConfig, StorageConfig};
use tokio_util::sync::CancellationToken;

pub use template::annotated_config;

/// gRPC port `--dev` listens on.
pub const DEV_GRPC_PORT: u16 = 7470;
/// HTTP port `--dev` listens on.
pub const DEV_HTTP_PORT: u16 = 7471;

/// Rill server.
#[derive(Parser, Debug)]
#[command(
    name = "rill-server",
    version,
    about = "The Rill server: gRPC API, object protocol and background workers"
)]
pub struct ServerArgs {
    /// Configuration file (TOML). `RILL_*` variables override it.
    #[arg(long, env = "RILL_CONFIG", global = true)]
    pub config: Option<PathBuf>,

    /// Development mode: in-memory storage, no authentication, ports
    /// 7470 (gRPC) and 7471 (HTTP). Never expose such a server.
    #[arg(long)]
    pub dev: bool,

    /// gRPC listen address.
    #[arg(long, value_name = "ADDR")]
    pub listen_grpc: Option<SocketAddr>,

    /// HTTP (object protocol, health) listen address.
    #[arg(long, value_name = "ADDR")]
    pub listen_http: Option<SocketAddr>,

    /// Storage: `memory` or `local:<dir>`.
    #[arg(long, value_name = "SPEC")]
    pub storage: Option<String>,

    /// Subcommands.
    #[command(subcommand)]
    pub command: Option<ServerCmd>,
}

/// `rill-server <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ServerCmd {
    /// Write an annotated default configuration file.
    InitConfig {
        /// Where to write it (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
}

/// Build the effective configuration: file (or defaults), then
/// environment, then flags.
pub fn build_config(args: &ServerArgs) -> anyhow::Result<ServerConfig> {
    let mut config = ServerConfig::load(args.config.as_deref())?;
    if args.dev {
        config.dev_mode = true;
        config.storage = StorageConfig::Memory;
        config.listen_grpc = SocketAddr::from(([127, 0, 0, 1], DEV_GRPC_PORT));
        config.listen_http = SocketAddr::from(([127, 0, 0, 1], DEV_HTTP_PORT));
    }
    if let Some(addr) = args.listen_grpc {
        config.listen_grpc = addr;
    }
    if let Some(addr) = args.listen_http {
        config.listen_http = addr;
    }
    if let Some(spec) = &args.storage {
        config.storage = parse_storage(spec)?;
    }
    Ok(config)
}

/// Parse `memory` or `local:<dir>`.
pub fn parse_storage(spec: &str) -> anyhow::Result<StorageConfig> {
    match spec.split_once(':') {
        None if spec == "memory" => Ok(StorageConfig::Memory),
        Some(("local", dir)) if !dir.is_empty() => Ok(StorageConfig::Local {
            dir: PathBuf::from(dir),
        }),
        _ => anyhow::bail!("--storage: expected `memory` or `local:<dir>`, got {spec:?}"),
    }
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// The binary's main: returns the process exit code.
pub fn main() -> std::process::ExitCode {
    let args = ServerArgs::parse();
    init_logging();
    match run(args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rill-server: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(args: ServerArgs) -> anyhow::Result<()> {
    if let Some(ServerCmd::InitConfig { out, force }) = &args.command {
        let text = annotated_config()?;
        match out {
            None => print!("{text}"),
            Some(path) => {
                if path.exists() && !force {
                    anyhow::bail!("{} exists; pass --force to overwrite", path.display());
                }
                std::fs::write(path, text)?;
                eprintln!("wrote {}", path.display());
            }
        }
        return Ok(());
    }
    let config = build_config(&args)?;
    if config.dev_mode {
        tracing::warn!("dev mode: no authentication, every request is permitted");
    }
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let shutdown = CancellationToken::new();
        let signal = shutdown.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                tracing::info!("ctrl-c received, shutting down");
                signal.cancel();
            }
        });
        rill_server::run(config, shutdown).await?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_flag_sets_memory_and_ports() {
        let args = ServerArgs::try_parse_from(["rill-server", "--dev"]).unwrap();
        let config = build_config(&args).unwrap();
        assert!(config.dev_mode);
        assert_eq!(config.storage, StorageConfig::Memory);
        assert_eq!(config.listen_grpc.port(), DEV_GRPC_PORT);
        assert_eq!(config.listen_http.port(), DEV_HTTP_PORT);
    }

    #[test]
    fn flags_override_dev_defaults() {
        let args = ServerArgs::try_parse_from([
            "rill-server",
            "--dev",
            "--listen-grpc",
            "127.0.0.1:0",
            "--storage",
            "local:/tmp/rill-data",
        ])
        .unwrap();
        let config = build_config(&args).unwrap();
        assert_eq!(config.listen_grpc.port(), 0);
        assert_eq!(config.listen_http.port(), DEV_HTTP_PORT);
        assert_eq!(
            config.storage,
            StorageConfig::Local {
                dir: PathBuf::from("/tmp/rill-data")
            }
        );
        assert!(parse_storage("s3:bucket").is_err());
        assert!(parse_storage("local:").is_err());
    }

    #[test]
    fn init_config_parses() {
        let args =
            ServerArgs::try_parse_from(["rill-server", "init-config", "--out", "x.toml"]).unwrap();
        assert!(matches!(
            args.command,
            Some(ServerCmd::InitConfig { force: false, .. })
        ));
    }
}
