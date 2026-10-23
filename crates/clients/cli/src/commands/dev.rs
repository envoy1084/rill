//! `rill dev`: the development server, which ships in `rill-server`. This
//! command finds that binary and becomes `rill-server dev`.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::Args;

use crate::error::CliError;
use crate::Result;

/// `rill dev`: every argument goes to `rill-server dev`.
#[derive(Args, Debug)]
#[command(disable_help_flag = true)]
pub struct DevArgs {
    /// Arguments for `rill-server dev` (`--data`, `--memory`,
    /// `--listen-grpc`, `--listen-http`, `--help`).
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "ARGS"
    )]
    pub args: Vec<OsString>,
}

const SERVER: &str = if cfg!(windows) {
    "rill-server.exe"
} else {
    "rill-server"
};

/// Replace this process with `rill-server dev <args>`.
pub fn run(args: DevArgs) -> Result<()> {
    let server = server_binary().ok_or_else(|| {
        CliError::usage(
            "`rill dev` runs `rill-server dev`, but rill-server is neither next to rill nor on PATH; \
             install it alongside rill",
        )
    })?;
    let mut command = std::process::Command::new(&server);
    command.arg("dev").args(&args.args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Only returns on failure; on success this process is the server.
        Err(CliError::Io(command.exec()))
    }
    #[cfg(not(unix))]
    {
        let status = command.status()?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// `rill-server` beside this executable, else the first on `PATH`.
fn server_binary() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(SERVER)))
        .filter(|p| p.is_file());
    beside.or_else(|| {
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(SERVER))
                .find(|p| p.is_file())
        })
    })
}
