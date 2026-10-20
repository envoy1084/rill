//! Actions: the unit of cached execution.
//!
//! An action is a pure function of its inputs: the command, its
//! environment, the input tree and the platform it runs on. Two actions
//! with the same digest produce the same result by definition, which is
//! what makes the result cache sound.

use std::collections::BTreeMap;
use std::time::Duration;

use rill_types::{
    ContentRef, Encodable, FormatVersion, ObjectHash, ObjectKind, RepoPathBuf, Timestamp, TreeHash,
};
use serde::{Deserialize, Serialize};

/// Exit code reported when a command exceeds its timeout, as `timeout(1)`
/// does.
pub const TIMEOUT_EXIT_CODE: i32 = 124;

/// A command to run against an input tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    /// Program and arguments; `command[0]` is the executable.
    pub command: Vec<String>,
    /// Environment the command sees, in addition to whatever the sandbox
    /// injects (`PATH`, `HOME`).
    pub env: BTreeMap<String, String>,
    /// Tree materialized as the working tree before the command runs.
    pub input_root: TreeHash,
    /// Directory within the input tree the command starts in.
    pub working_dir: RepoPathBuf,
    /// Wall-clock limit; the process is killed when it expires.
    pub timeout: Duration,
    /// Facts about the execution platform that affect the result (OS,
    /// architecture, toolchain). Part of the cache key.
    pub platform: BTreeMap<String, String>,
    /// Paths (relative to the input root) whose contents are captured
    /// into the result after the command exits.
    pub output_paths: Vec<RepoPathBuf>,
}

impl Encodable for Action {
    const KIND: ObjectKind = ObjectKind::Derived;
    const VERSION: FormatVersion = 1;
    const NAME: &'static str = "Action";
}

impl Action {
    /// A new action running `command` at the root of `input_root` with a
    /// ten-minute timeout and the host platform.
    pub fn new(command: Vec<String>, input_root: TreeHash) -> Self {
        Action {
            command,
            env: BTreeMap::new(),
            input_root,
            working_dir: RepoPathBuf::root(),
            timeout: Duration::from_secs(600),
            platform: host_platform(),
            output_paths: Vec::new(),
        }
    }

    /// Content hash of the canonical encoding: the cache key.
    pub fn digest(&self) -> ObjectHash {
        self.hash()
    }

    /// The executable, if the command is not empty.
    pub fn program(&self) -> Option<&str> {
        self.command.first().map(String::as_str)
    }
}

/// Platform facts of the current host: `os` and `arch`.
pub fn host_platform() -> BTreeMap<String, String> {
    let mut platform = BTreeMap::new();
    platform.insert("os".to_string(), std::env::consts::OS.to_string());
    platform.insert("arch".to_string(), std::env::consts::ARCH.to_string());
    platform
}

/// What an action produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResult {
    /// Process exit code; [`TIMEOUT_EXIT_CODE`] when the timeout expired.
    pub exit_code: i32,
    /// Captured standard output.
    pub stdout: ContentRef,
    /// Captured standard error.
    pub stderr: ContentRef,
    /// Wall-clock duration of the command.
    pub duration_ms: u64,
    /// Contents of the declared output paths that existed after the run.
    pub output_files: BTreeMap<RepoPathBuf, ContentRef>,
    /// Identity of the worker that executed the action.
    pub worker: String,
    /// When execution started.
    pub started_at: Timestamp,
    /// True when the process was killed for exceeding its timeout.
    pub timed_out: bool,
}

impl ActionResult {
    /// True when the command exited successfully.
    pub fn succeeded(&self) -> bool {
        self.exit_code == 0 && !self.timed_out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_independent_of_insertion_order() {
        let mut a = Action::new(vec!["true".into()], ObjectHash::ZERO);
        a.env.insert("Z".into(), "1".into());
        a.env.insert("A".into(), "2".into());
        a.platform.insert("zz".into(), "x".into());
        a.platform.insert("aa".into(), "y".into());

        let mut b = Action::new(vec!["true".into()], ObjectHash::ZERO);
        b.platform.insert("aa".into(), "y".into());
        b.platform.insert("zz".into(), "x".into());
        b.env.insert("A".into(), "2".into());
        b.env.insert("Z".into(), "1".into());

        assert_eq!(a.digest(), b.digest());
    }

    #[test]
    fn digest_changes_with_inputs() {
        let a = Action::new(vec!["true".into()], ObjectHash::ZERO);
        let mut b = a.clone();
        b.env.insert("K".into(), "v".into());
        assert_ne!(a.digest(), b.digest());
        let mut c = a.clone();
        c.timeout = Duration::from_secs(1);
        assert_ne!(a.digest(), c.digest());
    }
}
