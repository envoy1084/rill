//! Starting the daemon in the background when no one runs it.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::connect::DaemonClient;
use crate::config::DaemonDir;
use crate::error::{Error, Result};

/// How long a freshly started daemon has to start listening.
const START_TIMEOUT: Duration = Duration::from_secs(15);

/// Connect to the daemon of `dir`, first starting `program args…` in the
/// background (its own process group, output to the daemon log) when
/// none is running.
pub async fn connect_or_start(
    dir: &DaemonDir,
    program: &Path,
    args: &[&str],
) -> Result<DaemonClient> {
    if let Ok(client) = DaemonClient::connect(dir).await {
        return Ok(client);
    }
    std::fs::create_dir_all(dir.root())?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.log())?;
    let mut command = Command::new(program);
    command
        .args(args)
        .env(crate::config::DIR_ENV, dir.root())
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .spawn()
        .map_err(|e| Error::Unavailable(format!("starting {}: {e}", program.display())))?;
    let deadline = tokio::time::Instant::now() + START_TIMEOUT;
    loop {
        match DaemonClient::connect(dir).await {
            Ok(client) => return Ok(client),
            Err(e) if tokio::time::Instant::now() >= deadline => {
                return Err(Error::Unavailable(format!(
                    "the daemon did not start (see {}): {e}",
                    dir.log().display()
                )))
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
}
