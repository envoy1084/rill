//! Execution workers inside this server, per `[exec.local]`.

use std::sync::Arc;

use rill_exec::{Isolation, LocalSandbox, LocalWorker};
use tokio_util::sync::CancellationToken;

use crate::config::{LocalWorkersConfig, SandboxKind};
use crate::error::{Error, Result};
use crate::state::AppState;

/// Start `[exec.local] workers` workers, which stop with `shutdown`.
pub fn spawn_local_workers(state: &Arc<AppState>, shutdown: &CancellationToken) -> Result<()> {
    let local = &state.config.exec.local;
    if local.workers == 0 {
        return Ok(());
    }
    let isolation = isolation(local, state.config.dev_mode)?;
    let sandbox = Arc::new(LocalSandbox::new(local.network).with_isolation(isolation)?);
    let host = std::env::var("HOSTNAME")
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "localhost".into());
    let instance = rill_types::ids::random_id();
    for index in 0..local.workers {
        let worker = LocalWorker::new(
            state.exec.clone(),
            sandbox.clone(),
            format!("local@{host}/{}/{index}", hex::encode(&instance[..4])),
            &local.platform,
        );
        let shutdown = shutdown.clone();
        tokio::spawn(async move { worker.run(shutdown.cancelled_owned()).await });
    }
    tracing::info!(workers = local.workers, sandbox = ?local.sandbox, "local execution workers started");
    Ok(())
}

/// How `local` workers isolate commands. Only a development server may
/// run repository code unisolated.
fn isolation(local: &LocalWorkersConfig, dev_mode: bool) -> Result<Isolation> {
    Ok(match local.sandbox {
        SandboxKind::Local if !dev_mode => {
            return Err(Error::Config(
                "[exec.local] sandbox = \"local\" runs repository code unisolated; \
                 outside dev mode use \"confined\" or \"gvisor\""
                    .into(),
            ))
        }
        SandboxKind::Local => Isolation::None,
        SandboxKind::Confined => Isolation::Confined,
        SandboxKind::Gvisor => Isolation::Gvisor {
            runsc: local.runsc.clone().into(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unisolated_workers_need_dev_mode() {
        let local = LocalWorkersConfig {
            workers: 1,
            ..Default::default()
        };
        assert!(isolation(&local, false).is_err());
        assert_eq!(isolation(&local, true).unwrap(), Isolation::None);
        let gvisor = LocalWorkersConfig {
            sandbox: SandboxKind::Gvisor,
            ..local
        };
        assert!(matches!(
            isolation(&gvisor, false).unwrap(),
            Isolation::Gvisor { .. }
        ));
    }
}
