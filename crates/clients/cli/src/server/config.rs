//! The server configuration `rill dev` and `rill api` build from flags.

use std::net::SocketAddr;
use std::path::PathBuf;

use rill_api::{Roles, ServerConfig, StorageConfig};

use super::args::{ApiArgs, DevArgs};
use crate::error::CliError;
use crate::Result;

/// gRPC port `rill dev` listens on.
pub const DEV_GRPC_PORT: u16 = 7470;
/// HTTP port `rill dev` listens on.
pub const DEV_HTTP_PORT: u16 = 7471;
/// Execution workers `rill dev` runs in-process.
pub const DEV_LOCAL_WORKERS: usize = 2;

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
        // Plans run on workers in this process, unisolated: a development
        // server runs its user's own code, with the tools on its `PATH`.
        verify: rill_runtime::config::VerifyConfig {
            enabled: true,
            env: std::env::var("PATH")
                .map(|path| std::collections::BTreeMap::from([("PATH".to_string(), path)]))
                .unwrap_or_default(),
            ..Default::default()
        },
        exec: rill_runtime::config::ExecConfig {
            worker_secret_file: std::env::var_os("RILL_EXEC_WORKER_SECRET_FILE").map(PathBuf::from),
            local: rill_runtime::config::LocalWorkersConfig {
                workers: DEV_LOCAL_WORKERS,
                ..Default::default()
            },
            ..Default::default()
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
}
