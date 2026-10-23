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

mod aliases;
pub mod cli;
pub mod commands;
pub mod context;
pub mod credentials;
pub mod error;
pub mod exit;
pub mod output;
pub mod server;
pub mod settings;

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{CommandFactory, FromArgMatches};

use crate::settings::Settings;

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
    let settings = Arc::new(Settings::load());
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    match prepare(args, &settings, out) {
        Ok(invocation) => run(invocation, out, stdout_is_terminal(), settings).await,
        Err(code) => code,
    }
}

/// A parsed command line, ready to run.
struct Invocation {
    cli: Cli,
    schema: String,
}

/// Expand aliases and parse; help and version are printed here and
/// return their exit code, as do usage errors.
fn prepare(
    args: Vec<OsString>,
    settings: &Settings,
    out: &mut dyn Write,
) -> std::result::Result<Invocation, i32> {
    let builtins: Vec<String> = Cli::command()
        .get_subcommands()
        .flat_map(|c| {
            std::iter::once(c.get_name().to_string()).chain(c.get_all_aliases().map(String::from))
        })
        .collect();
    let args = aliases::expand(args, &settings.alias, &builtins);
    let parsed = Cli::command()
        .try_get_matches_from(args)
        .and_then(|m| Cli::from_arg_matches(&m).map(|cli| (cli, schema_of(&m))));
    match parsed {
        Ok((cli, schema)) => Ok(Invocation { cli, schema }),
        Err(e) => {
            // Help and version are "errors" to clap but successes to us.
            let code = if e.use_stderr() {
                exit::USAGE
            } else {
                exit::OK
            };
            if code == exit::OK {
                let text = if stdout_is_terminal() {
                    e.render().ansi().to_string()
                } else {
                    e.render().to_string()
                };
                let _ = out.write_all(text.as_bytes());
                let _ = out.flush();
            } else if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
                eprint!("{}", e.render().ansi());
            } else {
                eprint!("{}", e.render());
            }
            Err(code)
        }
    }
}

async fn run(
    invocation: Invocation,
    out: &mut dyn Write,
    terminal: bool,
    settings: Arc<Settings>,
) -> i32 {
    let Invocation { cli, schema } = invocation;
    let global = cli.global.clone();
    let json = global.json || global.json_schema.is_some();
    let color = !json && color_on(&global, &settings, terminal);
    let cwd = match &global.cwd {
        Some(dir) => dir.clone(),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let mut ctx = context::Ctx::new(global, out, color, cwd, schema, settings);
    match commands::run(&mut ctx, cli.command).await {
        Ok(()) => exit::OK,
        // The reader (a pager the user quit, `head`) went away.
        Err(CliError::Io(e)) if e.kind() == std::io::ErrorKind::BrokenPipe => exit::OK,
        Err(e) => {
            report_error(&e, json);
            exit::code_for(&e)
        }
    }
}

/// Whether to colour: `--no-color`/`--color`, then `NO_COLOR` and
/// `CLICOLOR_FORCE`, then `[color] ui`, then whether the output is a
/// terminal.
fn color_on(global: &cli::GlobalOpts, settings: &Settings, terminal: bool) -> bool {
    if global.no_color {
        return false;
    }
    let when = global
        .color
        .clone()
        .or_else(|| {
            if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
                Some("never".into())
            } else if std::env::var("CLICOLOR_FORCE").is_ok_and(|v| v != "0") {
                Some("always".into())
            } else {
                None
            }
        })
        .or_else(|| settings.color.ui.clone())
        .unwrap_or_else(|| "auto".into());
    match when.as_str() {
        "always" => true,
        "never" => false,
        _ => terminal,
    }
}

fn stdout_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// Commands whose output can run long and goes through the pager.
fn pages(command: &Command) -> bool {
    matches!(
        command,
        Command::Log(_)
            | Command::Diff(_)
            | Command::Show(_)
            | Command::Blame(_)
            | Command::Cat(_)
            | Command::Grep(_)
            | Command::Help(_)
    )
}

/// The JSON schema a command's output follows: `rill.cli.v1.` and the
/// command's path (`rill.cli.v1.review.list`).
fn schema_of(matches: &clap::ArgMatches) -> String {
    let mut schema = String::from(SCHEMA_PREFIX);
    let mut m = matches;
    while let Some((name, sub)) = m.subcommand() {
        schema.push('.');
        schema.push_str(&name.replace('-', "_"));
        m = sub;
    }
    schema
}

/// The JSON schema names' prefix: every `--json` document is
/// `{"schema": "rill.cli.v1.<command>", "result": …}`.
pub const SCHEMA_PREFIX: &str = "rill.cli.v1";

/// Print an error to stderr.
fn report_error(e: &CliError, json: bool) {
    if json {
        let value = serde_json::json!({
            "schema": format!("{SCHEMA_PREFIX}.error"),
            "error": e.to_json(),
        });
        eprintln!("{value}");
    } else {
        eprintln!("rill: {e}");
        for line in e.hints() {
            eprintln!("  {line}");
        }
    }
}

/// Logging to stderr controlled by `-v`/`-q` and `RUST_LOG`.
fn init_logging(verbose: u8, quiet: bool) {
    let default = match (quiet, verbose) {
        (true, _) => "off",
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
    // The service installs its own telemetry (`rill dev`, `rill api`).
    let serving = args
        .iter()
        .skip(1)
        .find(|a| !a.to_string_lossy().starts_with('-'))
        .is_some_and(|a| a == "dev" || a == "api");
    if !serving {
        init_logging(verbose, quiet);
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("rill: cannot start runtime: {e}");
            return std::process::ExitCode::from(exit::ERROR as u8);
        }
    };
    let settings = Arc::new(Settings::load());
    let stdout = std::io::stdout();
    let mut lock = std::io::BufWriter::new(stdout.lock());
    let invocation = match prepare(args, &settings, &mut lock) {
        Ok(i) => i,
        Err(code) => return std::process::ExitCode::from(code as u8),
    };
    let terminal = stdout_is_terminal();
    let global = &invocation.cli.global;
    let paged = terminal
        && !global.no_pager
        && !global.json
        && global.json_schema.is_none()
        && pages(&invocation.cli.command);
    let pager = paged
        .then(|| output::pager::command(&settings))
        .flatten()
        .and_then(|cmd| output::pager::Pager::start(&cmd));
    let code = match pager {
        Some(mut pager) => {
            let code = match pager.writer() {
                Some(w) => {
                    let mut buffered = std::io::BufWriter::new(w);
                    let code = runtime.block_on(run(invocation, &mut buffered, true, settings));
                    let _ = buffered.flush();
                    code
                }
                None => runtime.block_on(run(invocation, &mut lock, terminal, settings)),
            };
            pager.finish();
            code
        }
        None => runtime.block_on(run(invocation, &mut lock, terminal, settings)),
    };
    let _ = lock.flush();
    std::process::ExitCode::from(code as u8)
}
