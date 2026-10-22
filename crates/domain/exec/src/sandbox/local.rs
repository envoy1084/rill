//! Running actions as host processes, confined on Linux or under gVisor.

use std::path::{Path, PathBuf};
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

/// How a [`LocalSandbox`] isolates the processes it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Isolation {
    /// A cleared environment and a private directory, nothing more: for
    /// trusted single-tenant servers, tests and development on macOS.
    None,
    /// Linux only: Landlock, seccomp and resource limits (see `confine`).
    Confined,
    /// Each command under gVisor's `runsc do`, a user-space kernel.
    Gvisor {
        /// The `runsc` binary.
        runsc: PathBuf,
    },
}

/// Runs commands on the host with `tokio::process`, isolated per
/// [`Isolation`].
#[derive(Debug, Clone)]
pub struct LocalSandbox {
    /// Whether the command may reach the network. Enforced by `Confined`
    /// (TCP) and `Gvisor`; advisory under `Isolation::None`.
    pub network: bool,
    isolation: Isolation,
    worker: String,
}

impl Default for LocalSandbox {
    fn default() -> Self {
        LocalSandbox::new(true)
    }
}

impl LocalSandbox {
    /// An unisolated sandbox; `network` is advisory.
    pub fn new(network: bool) -> Self {
        let worker = std::env::var("HOSTNAME")
            .ok()
            .filter(|h| !h.is_empty())
            .map(|h| format!("local@{h}"))
            .unwrap_or_else(|| "local".to_string());
        LocalSandbox {
            network,
            isolation: Isolation::None,
            worker,
        }
    }

    /// Isolate commands with `isolation`; `Confined` is refused off Linux.
    pub fn with_isolation(mut self, isolation: Isolation) -> Result<Self> {
        if isolation == Isolation::Confined && !cfg!(target_os = "linux") {
            return Err(Error::NotSupported {
                backend: "confined",
                reason: "Landlock and seccomp need Linux".into(),
            });
        }
        self.isolation = isolation;
        Ok(self)
    }

    fn command(&self, dir: &Path, cwd: &Path, action: &Action) -> Result<Command> {
        let program = action
            .program()
            .ok_or_else(|| Error::InvalidAction("empty command".into()))?;
        let mut command = match &self.isolation {
            Isolation::Gvisor { runsc } => {
                let mut command = Command::new(runsc);
                let network = if self.network { "host" } else { "none" };
                command
                    .arg(format!("--network={network}"))
                    .arg("--overlay2=none")
                    .arg("do")
                    .arg(format!("--cwd={}", cwd.display()))
                    .arg("--")
                    .arg(program);
                command
            }
            _ => Command::new(program),
        };
        command.args(&action.command[1..]).current_dir(cwd);
        #[cfg(target_os = "linux")]
        if self.isolation == Isolation::Confined {
            let path: Vec<PathBuf> = std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default();
            let mut confinement = super::confine::Confinement::prepare(dir, &path, self.network)?;
            // SAFETY: `apply` only makes system calls (setrlimit,
            // landlock, prctl/seccomp) on state prepared before the fork;
            // it does not allocate or take locks.
            unsafe {
                command.pre_exec(move || confinement.apply());
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = dir;
        Ok(command)
    }
}

#[async_trait]
impl Sandbox for LocalSandbox {
    async fn run(&self, dir: &Path, action: &Action) -> Result<RawOutcome> {
        let program = action.program().unwrap_or_default();
        let cwd = local_path(dir, &action.working_dir)?;

        let mut command = self.command(dir, &cwd, action)?;
        command
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
        if self.isolation == Isolation::Confined && !action.env.contains_key("TMPDIR") {
            let tmp = dir.join(".rill-tmp");
            tokio::fs::create_dir_all(&tmp).await?;
            command.env("TMPDIR", tmp);
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

#[cfg(all(test, target_os = "linux"))]
mod confined_tests {
    use rill_types::ObjectHash;

    use super::*;

    #[tokio::test]
    async fn a_confined_step_writes_only_inside_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let target = outside.path().join("escaped");
        let script = format!(
            "echo in > inside && echo t > \"$TMPDIR/t\" && echo out > {} ; \
             test -e {} && echo LEAKED; cat inside",
            target.display(),
            target.display()
        );
        let action = Action::new(vec!["sh".into(), "-c".into(), script], ObjectHash::ZERO);
        let out = LocalSandbox::new(false)
            .with_isolation(Isolation::Confined)
            .unwrap()
            .run(dir.path(), &action)
            .await
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            stdout,
            "in\n",
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!target.exists());
    }
}
