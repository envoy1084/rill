//! The `rill` command line, including `rill dev` and `rill api`, which run
//! the service.
//!
//! The binary is thin: it calls [`run_cli`]. Everything else lives here so
//! tests can drive the CLI as a library with [`run_cli_with_args`] and
//! capture its output.
//!
//! Every command renders one JSON value (the same encoding the MCP server
//! uses, from [`rill_mcp::json`]) or a human view of it; `--json` picks
//! the former. Exit codes are documented in [`exit`].

#![warn(missing_docs)]

pub mod checkout_meta;
pub mod cli;
pub mod commands;
pub mod context;
pub mod error;
pub mod exit;
pub mod output;
pub mod server;

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

use clap::Parser;

pub use cli::{Cli, Command, GlobalOpts};
pub use error::CliError;

/// Result alias for commands.
pub type Result<T> = std::result::Result<T, CliError>;

/// Parse `args` (including the program name), run the command, write its
/// output to `out`, and return the exit code. Errors are printed to
/// stderr (as JSON with `--json`). Never panics on bad input.
pub async fn run_cli_with_args<I, T>(args: I, out: &mut dyn Write) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            // Help and version are "errors" to clap but successes to us.
            let code = if e.use_stderr() {
                exit::USAGE
            } else {
                exit::OK
            };
            let text = e.render().to_string();
            if code == exit::OK {
                let _ = out.write_all(text.as_bytes());
                let _ = out.flush();
            } else {
                eprint!("{text}");
            }
            return code;
        }
    };
    let global = cli.global.clone();
    let color = !global.no_color && !global.json && color_enabled();
    let cwd = match &global.cwd {
        Some(dir) => dir.clone(),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let json = global.json;
    let mut ctx = context::Ctx::new(global, out, color, cwd);
    match commands::run(&mut ctx, cli.command).await {
        Ok(()) => exit::OK,
        Err(e) => {
            report_error(&e, json);
            exit::code_for(&e)
        }
    }
}

/// Print an error to stderr.
fn report_error(e: &CliError, json: bool) {
    if json {
        let value = serde_json::json!({
            "error": { "code": e.code(), "message": e.to_string(), "exit": exit::code_for(e) }
        });
        eprintln!("{value}");
    } else {
        eprintln!("rill: {e}");
    }
}

/// True when stdout is a terminal and `NO_COLOR` is unset.
fn color_enabled() -> bool {
    use std::io::IsTerminal;
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

/// Logging to stderr controlled by `-v`/`-q` and `RUST_LOG`.
fn init_logging(verbose: u8, quiet: bool, serving: bool) {
    let default = match (quiet, verbose) {
        (true, _) => "off",
        (false, 0) if serving => "info",
        (false, 0) => "warn",
        (false, 1) => "info",
        (false, 2) => "debug",
        (false, _) => "trace",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .without_time()
        .try_init();
}

/// Entry point of the `rill` binary: parse the process arguments, run,
/// and return the exit code.
pub fn run_cli() -> std::process::ExitCode {
    // Peek at the global logging flags before clap so logging covers
    // parsing too; clap re-parses them properly afterwards.
    let args: Vec<OsString> = std::env::args_os().collect();
    let verbose = args
        .iter()
        .filter(|a| {
            let s = a.to_string_lossy();
            s == "-v"
                || s == "--verbose"
                || (s.starts_with("-v") && s[1..].chars().all(|c| c == 'v'))
        })
        .map(|a| a.to_string_lossy().matches('v').count() as u8)
        .sum();
    let quiet = args.iter().any(|a| a == "-q" || a == "--quiet");
    // The service logs what it does by default; client commands stay quiet.
    let serving = args
        .iter()
        .skip(1)
        .find(|a| !a.to_string_lossy().starts_with('-'))
        .is_some_and(|a| a == "dev" || a == "api");
    init_logging(verbose, quiet, serving);

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("rill: cannot start runtime: {e}");
            return std::process::ExitCode::from(exit::ERROR as u8);
        }
    };
    let stdout = std::io::stdout();
    let mut lock = std::io::BufWriter::new(stdout.lock());
    let code = runtime.block_on(run_cli_with_args(args, &mut lock));
    let _ = lock.flush();
    std::process::ExitCode::from(code as u8)
}
