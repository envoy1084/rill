//! Paging long output, like git: through `$RILL_PAGER`, `[core] pager`,
//! `$PAGER`, else `less` with `LESS=FRX` (quit if one screen, keep colour,
//! don't clear) when `LESS` is unset.

use std::process::{Child, ChildStdin, Command, Stdio};

use crate::settings::Settings;

/// The pager command to run, or `None` when paging is off (`cat` or an
/// empty command).
pub fn command(settings: &Settings) -> Option<String> {
    let chosen = std::env::var("RILL_PAGER")
        .ok()
        .or_else(|| settings.core.pager.clone())
        .or_else(|| std::env::var("PAGER").ok())
        .unwrap_or_else(|| "less".into());
    let trimmed = chosen.trim();
    (!trimmed.is_empty() && trimmed != "cat").then(|| trimmed.to_string())
}

/// A running pager reading what the command prints.
pub struct Pager {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Pager {
    /// Start `command`; `None` when it cannot run (output then goes to
    /// stdout directly).
    pub fn start(command: &str) -> Option<Pager> {
        let words = shlex::split(command)?;
        let (program, args) = words.split_first()?;
        let mut cmd = Command::new(program);
        cmd.args(args).stdin(Stdio::piped());
        if std::env::var_os("LESS").is_none() {
            cmd.env("LESS", "FRX");
        }
        if std::env::var_os("LV").is_none() {
            cmd.env("LV", "-c");
        }
        let mut child = cmd.spawn().ok()?;
        let stdin = child.stdin.take();
        Some(Pager { child, stdin })
    }

    /// Where to write the output.
    pub fn writer(&mut self) -> Option<&mut ChildStdin> {
        self.stdin.as_mut()
    }

    /// Close the pager's input and wait for the user to quit it.
    pub fn finish(mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}
