//! `rill dev` and `rill api`: running the service.
//!
//! `rill dev` runs every role in one process for a laptop: no
//! authentication, storage in a local directory (or memory), fixed ports.
//! `rill api` runs a production process from a configuration file and
//! `RILL_*` variables, with `--roles` choosing what it does; any number of
//! such processes may share one store.

mod template;

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use rill_api::{Roles, ServerConfig, StorageConfig};
use tokio_util::sync::CancellationToken;

pub use template::annotated_config;

use crate::error::CliError;
use crate::Result;

/// gRPC port `rill dev` listens on.
pub const DEV_GRPC_PORT: u16 = 7470;
/// HTTP port `rill dev` listens on.
pub const DEV_HTTP_PORT: u16 = 7471;

/// `rill dev`.
#[derive(Args, Debug)]
pub struct DevArgs {
    /// Directory for objects and metadata (default: `~/.rill/dev`).
    #[arg(long, value_name = "DIR", conflicts_with = "memory")]
    pub data: Option<PathBuf>,
    /// Keep everything in memory; nothing survives the process.
    #[arg(long)]
    pub memory: bool,
    /// gRPC listen address (default: 127.0.0.1:7470).
    #[arg(long, value_name = "ADDR")]
    pub listen_grpc: Option<SocketAddr>,
    /// HTTP listen address (default: 127.0.0.1:7471).
    #[arg(long, value_name = "ADDR")]
    pub listen_http: Option<SocketAddr>,
}

/// `rill api`.
#[derive(Args, Debug)]
pub struct ApiArgs {
    /// Configuration file (TOML). `RILL_*` variables override it.
    #[arg(long, env = "RILL_CONFIG")]
    pub config: Option<PathBuf>,
    /// gRPC listen address.
    #[arg(long, value_name = "ADDR")]
    pub listen_grpc: Option<SocketAddr>,
    /// HTTP (object protocol, health) listen address.
    #[arg(long, value_name = "ADDR")]
    pub listen_http: Option<SocketAddr>,
    /// Storage: `memory` or `local:<dir>`.
    #[arg(long, value_name = "SPEC")]
    pub storage: Option<String>,
    /// What this process runs: `api`, `workers` or `api,workers`.
    #[arg(long, value_name = "ROLES")]
    pub roles: Option<String>,
    /// Subcommands.
    #[command(subcommand)]
    pub command: Option<ApiCmd>,
}

/// `rill api <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ApiCmd {
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

/// The configuration `rill dev` runs with.
pub fn dev_config(args: &DevArgs) -> Result<ServerConfig> {
    let storage = if args.memory {
        StorageConfig::Memory
    } else {
        let dir = match &args.data {
            Some(dir) => dir.clone(),
            None => default_dev_dir()?,
        };
        StorageConfig::Local { dir }
    };
    Ok(ServerConfig {
        dev_mode: true,
        storage,
        listen_grpc: args
            .listen_grpc
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], DEV_GRPC_PORT))),
        listen_http: args
            .listen_http
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], DEV_HTTP_PORT))),
        ..ServerConfig::default()
    })
}

/// The configuration `rill api` runs with: file (or defaults), then
/// environment, then flags.
pub fn api_config(args: &ApiArgs) -> Result<ServerConfig> {
    let mut config = ServerConfig::load(args.config.as_deref())?;
    if let Some(addr) = args.listen_grpc {
        config.listen_grpc = addr;
    }
    if let Some(addr) = args.listen_http {
        config.listen_http = addr;
    }
    if let Some(spec) = &args.storage {
        config.storage = parse_storage(spec)?;
    }
    if let Some(spec) = &args.roles {
        config.roles = Roles::parse(spec)?;
    }
    Ok(config)
}

/// Parse `memory` or `local:<dir>`.
pub fn parse_storage(spec: &str) -> Result<StorageConfig> {
    match spec.split_once(':') {
        None if spec == "memory" => Ok(StorageConfig::Memory),
        Some(("local", dir)) if !dir.is_empty() => Ok(StorageConfig::Local {
            dir: PathBuf::from(dir),
        }),
        _ => Err(CliError::Usage(format!(
            "--storage: expected `memory` or `local:<dir>`, got {spec:?}"
        ))),
    }
}

/// Run `rill dev` until ctrl-c.
pub async fn dev(args: DevArgs) -> Result<()> {
    let config = dev_config(&args)?;
    tracing::warn!(storage = ?config.storage, "dev mode: no authentication, every request is permitted");
    serve(config).await
}

/// Run `rill api` until ctrl-c, or write the configuration template.
pub async fn api(args: ApiArgs) -> Result<()> {
    if let Some(ApiCmd::InitConfig { out, force }) = &args.command {
        return write_template(out.as_deref(), *force);
    }
    let config = api_config(&args)?;
    if config.dev_mode {
        tracing::warn!("dev mode: no authentication, every request is permitted");
    }
    serve(config).await
}

async fn serve(config: ServerConfig) -> Result<()> {
    let shutdown = CancellationToken::new();
    let signal = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("ctrl-c received, shutting down");
            signal.cancel();
        }
    });
    Ok(rill_api::run(config, shutdown).await?)
}

fn write_template(out: Option<&std::path::Path>, force: bool) -> Result<()> {
    let text = annotated_config()?;
    match out {
        None => print!("{text}"),
        Some(path) => {
            if path.exists() && !force {
                return Err(CliError::Usage(format!(
                    "{} exists; pass --force to overwrite",
                    path.display()
                )));
            }
            std::fs::write(path, text)?;
            eprintln!("wrote {}", path.display());
        }
    }
    Ok(())
}

fn default_dev_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| CliError::Usage("HOME is not set; pass --data or --memory".into()))?;
    Ok(PathBuf::from(home).join(".rill").join("dev"))
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::{Cli, Command};

    fn parse(args: &[&str]) -> Command {
        Cli::try_parse_from(args).unwrap().command
    }

    #[test]
    fn dev_runs_every_role_on_fixed_ports() {
        let Command::Dev(args) = parse(&["rill", "dev", "--data", "/tmp/rill-dev"]) else {
            panic!("not dev");
        };
        let config = dev_config(&args).unwrap();
        assert!(config.dev_mode);
        assert_eq!(config.roles, Roles::default());
        assert_eq!(
            config.storage,
            StorageConfig::Local {
                dir: PathBuf::from("/tmp/rill-dev")
            }
        );
        assert_eq!(config.listen_grpc.port(), DEV_GRPC_PORT);
        let Command::Dev(args) = parse(&["rill", "dev", "--memory"]) else {
            panic!("not dev");
        };
        assert_eq!(dev_config(&args).unwrap().storage, StorageConfig::Memory);
    }

    #[test]
    fn api_flags_override_the_file() {
        let Command::Api(args) = parse(&[
            "rill",
            "api",
            "--listen-grpc",
            "127.0.0.1:0",
            "--storage",
            "local:/tmp/rill-data",
            "--roles",
            "workers",
        ]) else {
            panic!("not api");
        };
        let config = api_config(&args).unwrap();
        assert!(!config.dev_mode);
        assert_eq!(config.listen_grpc.port(), 0);
        assert_eq!(
            config.roles,
            Roles {
                api: false,
                workers: true
            }
        );
        assert!(parse_storage("s3:bucket").is_err());
        assert!(Roles::parse("gateway").is_err());
        assert!(Roles::parse("").is_err());
    }

    #[test]
    fn init_config_parses() {
        let Command::Api(args) = parse(&["rill", "api", "init-config", "--out", "x.toml"]) else {
            panic!("not api");
        };
        assert!(matches!(
            args.command,
            Some(ApiCmd::InitConfig { force: false, .. })
        ));
    }
}
