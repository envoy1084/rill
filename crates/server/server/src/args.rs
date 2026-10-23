//! The command line of `rill-server`.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::cache::CacheArgs;
use crate::import::ImportCmd;

/// Rill's services: the API and workers, the dev server and the cache tier.
#[derive(Parser, Debug)]
#[command(name = "rill-server", version, about, propagate_version = true)]
pub struct Cli {
    /// What to run.
    #[command(subcommand)]
    pub command: Command,
}

/// A `rill-server` subcommand.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run the services from a configuration file and `RILL_*` variables.
    Run(ServeArgs),
    /// Run every role in one process for a laptop: no authentication,
    /// storage in a directory (or memory), fixed ports.
    Dev(DevArgs),
    /// Run the regional cache tier in front of an origin.
    Cache(CacheArgs),
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
    MintToken(MintArgs),
    /// Import history straight into a storage directory no server has open.
    #[command(subcommand)]
    Import(ImportCmd),
}

/// `rill-server dev`.
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

/// The configuration `run` and `mint-token` read.
#[derive(Args, Debug)]
pub struct ServeArgs {
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
}

/// `rill-server mint-token`.
#[derive(Args, Debug)]
pub struct MintArgs {
    /// `human:<name>`, `agent:<name>` or `service:<name>`; a bare name is a
    /// human.
    pub principal: String,
    /// Organization the token belongs to.
    #[arg(long, default_value = rill_store::DEFAULT_ORG)]
    pub org: String,
    /// Lifetime in seconds (default: the longest the kind allows).
    #[arg(long, value_name = "SECS")]
    pub ttl: Option<u64>,
    /// The instance's configuration.
    #[command(flatten)]
    pub serve: ServeArgs,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcommands_parse() {
        let cli = Cli::try_parse_from(["rill-server", "init-config", "--out", "x.toml"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::InitConfig { force: false, .. }
        ));
        let cli = Cli::try_parse_from(["rill-server", "mint-token", "root", "--config", "c.toml"])
            .unwrap();
        let Command::MintToken(m) = cli.command else {
            panic!("not mint-token");
        };
        assert_eq!(
            m.serve.config.as_deref(),
            Some(std::path::Path::new("c.toml"))
        );
    }
}
