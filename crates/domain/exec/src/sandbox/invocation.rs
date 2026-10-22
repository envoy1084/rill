//! What a sandbox is asked to run.

use std::collections::BTreeMap;
use std::time::Duration;

use rill_types::RepoPathBuf;

/// A command to run in a staged directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// Program and arguments; `command[0]` is the executable.
    pub command: Vec<String>,
    /// Environment the command sees, besides what the sandbox passes
    /// through (`PATH`, `HOME`).
    pub env: BTreeMap<String, String>,
    /// Directory within the staged tree the command starts in.
    pub working_dir: RepoPathBuf,
    /// Wall-clock limit; the process group is killed when it expires.
    pub timeout: Duration,
}

impl Invocation {
    /// `command` at the root with no environment and a ten-minute timeout.
    pub fn new(command: Vec<String>) -> Self {
        Invocation {
            command,
            env: BTreeMap::new(),
            working_dir: RepoPathBuf::root(),
            timeout: Duration::from_secs(600),
        }
    }

    /// The executable, if the command is not empty.
    pub fn program(&self) -> Option<&str> {
        self.command.first().map(String::as_str)
    }
}
