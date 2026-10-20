//! Running actions as ordinary host processes.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::task::JoinHandle;

use super::{RawOutcome, Sandbox};
use crate::action::{Action, TIMEOUT_EXIT_CODE};
use crate::error::{Error, Result};
use crate::materialize::local_path;

/// Host variables passed through to every command, on top of the
/// action's own environment.
const PASSTHROUGH_ENV: &[&str] = &["PATH", "HOME"];

/// Runs commands directly on the host with `tokio::process`.
///
/// Isolation is limited to a cleared environment and a private working
/// directory; this backend is for trusted single-tenant deployments and
/// tests. Multi-tenant servers should use a VM-backed sandbox.
#[derive(Debug, Clone)]
pub struct LocalSandbox {
    /// Whether the command may reach the network. Not enforced here: the
    /// host process has whatever network the server has. Recorded so
    /// callers can express the policy that a stronger backend enforces.
    pub network: bool,
    worker: String,
}

impl Default for LocalSandbox {
    fn default() -> Self {
        LocalSandbox::new(true)
    }
}

impl LocalSandbox {
    /// A local sandbox; `network` is advisory (see the field docs).
    pub fn new(network: bool) -> Self {
        let worker = std::env::var("HOSTNAME")
            .ok()
            .filter(|h| !h.is_empty())
            .map(|h| format!("local@{h}"))
            .unwrap_or_else(|| "local".to_string());
        LocalSandbox { network, worker }
    }
}

#[async_trait]
impl Sandbox for LocalSandbox {
    async fn run(&self, dir: &Path, action: &Action) -> Result<RawOutcome> {
        let program = action
            .program()
            .ok_or_else(|| Error::InvalidAction("empty command".into()))?;
        let cwd = local_path(dir, &action.working_dir)?;

        let mut command = Command::new(program);
        command
            .args(&action.command[1..])
            .current_dir(&cwd)
            .env_clear()
            .envs(&action.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for name in PASSTHROUGH_ENV {
            if action.env.contains_key(*name) {
                continue;
            }
            if let Ok(value) = std::env::var(name) {
                command.env(name, value);
            }
        }

        // The command gets its own process group so a timeout kills its
        // descendants too; otherwise an orphaned grandchild keeps the
        // output pipes open and the drain below never finishes.
        #[cfg(unix)]
        command.process_group(0);

        let started = Instant::now();
        let mut child = command.spawn().map_err(|source| Error::Spawn {
            command: program.to_string(),
            source,
        })?;
        let pid = child.id();
        // Readers run concurrently with the wait so a chatty process never
        // fills its pipe and deadlocks against us.
        let stdout = tokio::spawn(drain(child.stdout.take()));
        let stderr = tokio::spawn(drain(child.stderr.take()));

        let (exit_code, timed_out) = match tokio::time::timeout(action.timeout, child.wait()).await
        {
            Ok(status) => (status?.code().unwrap_or(-1), false),
            Err(_) => {
                tracing::warn!(program, timeout = ?action.timeout, "action timed out; killing");
                kill_group(pid);
                // The process may already have exited between the timeout
                // firing and the kill; either way it is gone.
                let _ = child.kill().await;
                (TIMEOUT_EXIT_CODE, true)
            }
        };
        let duration = started.elapsed();
        let stdout = finish_drain(stdout, timed_out).await?;
        let stderr = finish_drain(stderr, timed_out).await?;
        Ok(RawOutcome {
            exit_code,
            stdout,
            stderr,
            duration,
            timed_out,
        })
    }

    fn worker_name(&self) -> String {
        self.worker.clone()
    }
}

/// How long to wait for the output pipes to close after a kill. Something
/// outside the process group (a daemon the command started with `setsid`)
/// may still hold them; its output is not ours to wait for.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

async fn drain<R: AsyncRead + Unpin>(reader: Option<R>) -> std::io::Result<Bytes> {
    let mut buf = Vec::new();
    if let Some(mut reader) = reader {
        reader.read_to_end(&mut buf).await?;
    }
    Ok(Bytes::from(buf))
}

async fn finish_drain(task: JoinHandle<std::io::Result<Bytes>>, timed_out: bool) -> Result<Bytes> {
    if !timed_out {
        return Ok(task.await??);
    }
    match tokio::time::timeout(DRAIN_GRACE, task).await {
        Ok(result) => Ok(result??),
        Err(_) => Ok(Bytes::new()),
    }
}

#[cfg(unix)]
fn kill_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        // SAFETY: killpg is a plain syscall with no memory arguments; a
        // stale pid at worst targets a group that no longer exists, which
        // returns ESRCH.
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
fn kill_group(_pid: Option<u32>) {}

#[cfg(all(test, unix))]
mod tests {
    use rill_types::ObjectHash;

    use super::*;

    fn sh(script: &str) -> Action {
        Action::new(
            vec!["sh".into(), "-c".into(), script.into()],
            ObjectHash::ZERO,
        )
    }

    #[tokio::test]
    async fn captures_output_and_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let out = LocalSandbox::default()
            .run(dir.path(), &sh("echo hi; echo err >&2; exit 3"))
            .await
            .unwrap();
        assert_eq!(out.exit_code, 3);
        assert_eq!(out.stdout.as_ref(), b"hi\n");
        assert_eq!(out.stderr.as_ref(), b"err\n");
        assert!(!out.timed_out);
    }

    #[tokio::test]
    async fn environment_is_only_the_action_plus_passthrough() {
        let dir = tempfile::tempdir().unwrap();
        let mut action = sh("echo \"$RILL_X:$PATH\"; env | grep -c ^CARGO_ || true");
        action.env.insert("RILL_X".into(), "yes".into());
        let out = LocalSandbox::default()
            .run(dir.path(), &action)
            .await
            .unwrap();
        let text = String::from_utf8(out.stdout.to_vec()).unwrap();
        let mut lines = text.lines();
        assert!(lines.next().unwrap().starts_with("yes:"));
        assert_eq!(lines.next().unwrap().trim(), "0");
    }

    #[tokio::test]
    async fn kills_on_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let mut action = sh("echo start; sleep 30; echo done");
        action.timeout = Duration::from_millis(200);
        let started = Instant::now();
        let out = LocalSandbox::default()
            .run(dir.path(), &action)
            .await
            .unwrap();
        assert!(out.timed_out);
        assert_eq!(out.exit_code, TIMEOUT_EXIT_CODE);
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(out.stdout.as_ref(), b"start\n");
    }
}
