//! Command-line arguments of `rill dev` and `rill api`.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Args, Subcommand};

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

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::{Cli, Command};

    fn parse(args: &[&str]) -> Command {
        Cli::try_parse_from(args).unwrap().command
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
