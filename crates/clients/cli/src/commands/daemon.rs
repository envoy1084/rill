//! `rill daemon`: run, start, stop and inspect the client daemon.

use std::sync::Arc;

use clap::Subcommand;
use rill_daemon::proto as dpb;
use rill_daemon::{Daemon, DaemonClient, DaemonConfig};
use serde_json::json as j;

use crate::context::Ctx;
use crate::output::Table;
use crate::Result;

/// `rill daemon <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum DaemonCmd {
    /// Run the daemon in the foreground.
    Run,
    /// Start the daemon in the background unless it runs.
    Start,
    /// Stop the daemon.
    Stop,
    /// The daemon's cache and mounts.
    Status,
    /// Stop serving a checkout (a lazy one disappears; an eager one's
    /// files stay).
    Unmount {
        /// The checkout directory.
        dir: std::path::PathBuf,
        /// Unmount even with edits not synced to the server (they are lost).
        #[arg(long)]
        force: bool,
    },
}

/// Run `rill daemon`.
pub async fn run(ctx: &mut Ctx<'_>, cmd: DaemonCmd) -> Result<()> {
    match cmd {
        DaemonCmd::Run => {
            let mut config = DaemonConfig::new(ctx.daemon_dir()?);
            config.clients = Arc::new(crate::credentials::daemon_client_config);
            let daemon = Daemon::open(config).await?;
            let stop = daemon.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    stop.stop();
                }
            });
            Ok(rill_daemon::serve(daemon).await?)
        }
        DaemonCmd::Start => {
            let daemon = ctx.daemon().await?;
            let info = daemon.get_daemon(dpb::GetDaemonRequest::default()).await?;
            ctx.emit(j!({ "pid": info.pid, "version": info.version }), |out| {
                out.line(format!("daemon running (pid {})", info.pid))
            })
        }
        DaemonCmd::Stop => {
            let dir = ctx.daemon_dir()?;
            let stopped = match DaemonClient::connect(&dir).await {
                Ok(daemon) => {
                    daemon.shutdown(dpb::ShutdownRequest::default()).await?;
                    true
                }
                Err(_) => false,
            };
            ctx.emit(j!({ "stopped": stopped }), |out| {
                out.line(if stopped {
                    "daemon stopped"
                } else {
                    "no daemon running"
                })
            })
        }
        DaemonCmd::Unmount { dir, force } => {
            let dir = if dir.is_absolute() {
                dir
            } else {
                ctx.cwd().join(dir)
            };
            let daemon = ctx.daemon().await?;
            let mount = daemon
                .get_mount(dpb::GetMountRequest {
                    path: dir.display().to_string(),
                    ..Default::default()
                })
                .await?
                .mount
                .into_option()
                .unwrap_or_default();
            if mount.pending > 0 && !force {
                let question = format!(
                    "{} has {} edit(s) not synced to the server; unmounting loses them. Unmount anyway?",
                    mount.path, mount.pending
                );
                if !ctx.confirm(&question, false)? {
                    return Err(crate::error::CliError::usage(
                        "not unmounted: edits are not synced yet (`rill daemon status`, or --force)",
                    ));
                }
            }
            daemon
                .delete_mount(dpb::DeleteMountRequest {
                    name: mount.name.clone(),
                })
                .await?;
            ctx.emit(j!({ "unmounted": mount.name, "dir": mount.path }), |out| {
                out.line(format!("unmounted {}", mount.path))
            })
        }
        DaemonCmd::Status => {
            let daemon = ctx.daemon().await?;
            let info = daemon.get_daemon(dpb::GetDaemonRequest::default()).await?;
            let mounts = daemon
                .list_mounts(dpb::ListMountsRequest::default())
                .await?
                .mounts;
            let cache = info.cache.clone().into_option().unwrap_or_default();
            ctx.emit(
                j!({
                    "pid": info.pid,
                    "version": info.version,
                    "cache": {
                        "objects": cache.objects, "bytes": cache.bytes,
                        "budget_bytes": cache.budget_bytes, "hits": cache.hits,
                        "misses": cache.misses, "evictions": cache.evictions,
                    },
                    "mounts": mounts.iter().map(|m| j!({
                        "name": m.name, "path": m.path, "repo": m.repo,
                        "base": m.base, "pending": m.pending, "sync_error": m.sync_error,
                    })).collect::<Vec<_>>(),
                }),
                |out| {
                    out.line(format!(
                        "daemon pid {}: cache {} objects, {} of {} MiB, {} hits, {} misses",
                        info.pid,
                        cache.objects,
                        cache.bytes >> 20,
                        cache.budget_bytes >> 20,
                        cache.hits,
                        cache.misses
                    ))?;
                    let mut table = Table::new(&["PATH", "REPO", "BASE", "PENDING", "SYNC"]);
                    for m in &mounts {
                        table.row(vec![
                            m.path.clone(),
                            m.repo.clone(),
                            m.base.clone(),
                            m.pending.to_string(),
                            if m.sync_error.is_empty() {
                                "ok".into()
                            } else {
                                m.sync_error.clone()
                            },
                        ]);
                    }
                    out.table(table)
                },
            )
        }
    }
}
