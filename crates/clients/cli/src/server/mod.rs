//! `rill dev` and `rill api`: running the service.
//!
//! `rill dev` runs every role in one process for a laptop: no
//! authentication, storage in a local directory (or memory), fixed ports.
//! `rill api` runs a production process from a configuration file and
//! `RILL_*` variables, with `--roles` choosing what it does; any number of
//! such processes may share one store.

pub mod cache;
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
    /// Sign a token with the instance's key, reading the store and key
    /// service from the configuration. For bootstrap: the first
    /// administrator, service accounts, staging.
    MintToken {
        /// `human:<name>`, `agent:<name>` or `service:<name>`; a bare name
        /// is a human.
        principal: String,
        /// Organization the token belongs to.
        #[arg(long, default_value = rill_store::DEFAULT_ORG)]
        org: String,
        /// Lifetime in seconds (default: the longest the kind allows).
        #[arg(long, value_name = "SECS")]
        ttl: Option<u64>,
    },
}

/// Parse `kind:name` or `name`.
pub fn parse_principal(spec: &str) -> Result<rill_types::Principal> {
    use rill_types::Principal;
    match spec.split_once(':') {
        None => Ok(Principal::human(spec)),
        Some(("human", name)) => Ok(Principal::human(name)),
        Some(("agent", name)) => Ok(Principal::agent(name)),
        Some(("service", name)) => Ok(Principal::service(name)),
        Some((kind, _)) => Err(CliError::Usage(format!(
            "principal kind must be human, agent or service, not {kind:?}"
        ))),
    }
}

/// The configuration `rill dev` runs with.
pub fn dev_config(args: &DevArgs) -> Result<ServerConfig> {
    let (storage, master_key_file) = if args.memory {
        (StorageConfig::Memory, None)
    } else {
        let dir = match &args.data {
            Some(dir) => dir.clone(),
            None => default_dev_dir()?,
        };
        let key = dir.join("master.key");
        (StorageConfig::Local { dir }, Some(key))
    };
    let identity = rill_runtime::config::IdentityConfig {
        keys: rill_identity::keys::KeyServiceConfig::Local {
            master_key_hex: None,
            master_key_file,
        },
        ..Default::default()
    };
    Ok(ServerConfig {
        dev_mode: true,
        storage,
        identity,
        listen_grpc: args
            .listen_grpc
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], DEV_GRPC_PORT))),
        listen_http: args
            .listen_http
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], DEV_HTTP_PORT))),
        // No load balancer to drain from.
        shutdown: rill_api::ShutdownConfig {
            drain_secs: 0,
            grace_secs: 5,
        },
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
    if let rill_identity::keys::KeyServiceConfig::Local {
        master_key_file: Some(path),
        ..
    } = &config.identity.keys
    {
        ensure_master_key(path).await?;
    }
    serve(config).await
}

/// Create the dev server's master key file the first time, so its token
/// keys open again after a restart.
async fn ensure_master_key(path: &std::path::Path) -> Result<()> {
    if tokio::fs::try_exists(path).await? {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let key = hex::encode(rand::random::<[u8; 32]>());
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    use tokio::io::AsyncWriteExt;
    options.open(path).await?.write_all(key.as_bytes()).await?;
    Ok(())
}

/// Run `rill api` until ctrl-c, or write the configuration template.
pub async fn api(ctx: &mut crate::context::Ctx<'_>, args: ApiArgs) -> Result<()> {
    match &args.command {
        Some(ApiCmd::InitConfig { out, force }) => write_template(out.as_deref(), *force),
        Some(ApiCmd::MintToken {
            principal,
            org,
            ttl,
        }) => {
            let config = api_config(&args)?;
            let principal = parse_principal(principal)?;
            let token = rill_runtime::identity::mint_token(&config, &principal, org, *ttl).await?;
            ctx.emit(serde_json::json!({ "token": token }), |out| {
                out.line(&token)
            })
        }
        None => serve(api_config(&args)?).await,
    }
}

async fn serve(config: ServerConfig) -> Result<()> {
    let _telemetry = rill_runtime::telemetry::init(&config.telemetry)?;
    if config.dev_mode {
        tracing::warn!(storage = ?config.storage, "dev mode: no authentication, every request is permitted");
    }
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
    fn principals_parse_by_kind() {
        assert_eq!(
            parse_principal("alice").unwrap(),
            rill_types::Principal::human("alice")
        );
        assert_eq!(
            parse_principal("service:load").unwrap(),
            rill_types::Principal::service("load")
        );
        assert!(parse_principal("robot:x").is_err());
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
