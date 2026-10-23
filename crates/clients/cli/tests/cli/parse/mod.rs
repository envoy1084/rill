//! Command-line parsing for every command group.

mod globals;
mod local;
mod management;
mod review;

use clap::Parser;
use rill_cli::cli::Cli;

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["rill"];
    full.extend_from_slice(args);
    Cli::try_parse_from(full).unwrap_or_else(|e| panic!("{args:?}: {e}"))
}
