//! `rill-server`: Rill's services and the tools that work on their
//! storage directly.
//!
//! `run` starts the API and workers from a configuration file; `dev` runs
//! every role on a laptop; `cache` is the regional cache tier;
//! `init-config`, `mint-token` and `import git` act on a store no server
//! need be running for. The `rill` client never links any of this.

#![warn(missing_docs)]

pub mod args;
pub mod cache;
pub mod config;
pub mod error;
pub mod import;
pub mod principal;
pub mod run;
pub mod template;

use std::ffi::OsString;
use std::io::Write;

use clap::Parser;

pub use error::{Error, Result};

use crate::args::{Cli, Command};

/// Exit code of a usage error.
pub const USAGE: i32 = 2;

/// Parse `args` (including the program name), run the command, write its
/// output to `out`, and return the exit code.
pub async fn run_with_args<I, T>(args: I, out: &mut dyn Write) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            let code = if e.use_stderr() { USAGE } else { 0 };
            if code == 0 {
                let _ = write!(out, "{}", e.render());
            } else {
                eprint!("{}", e.render());
            }
            return code;
        }
    };
    let result = match cli.command {
        Command::Run(args) => run::run(args).await,
        Command::Dev(args) => run::dev(args).await,
        Command::Cache(args) => cache::run(args).await,
        Command::InitConfig { out: path, force } => {
            run::write_template(path.as_deref(), force, out)
        }
        Command::MintToken(args) => run::mint(args, out).await,
        Command::Import(cmd) => import::run(cmd, out).await,
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("rill-server: {e}");
            if matches!(e, Error::Usage(_)) {
                USAGE
            } else {
                1
            }
        }
    }
}
