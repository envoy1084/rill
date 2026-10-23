//! Serving the daemon on its local socket, readable only by its owner.
//! The daemon stops when asked, or when its socket file disappears (its
//! directory was removed).

use std::sync::Arc;
use std::time::Duration;

use super::service::DaemonSvc;
use crate::daemon::Daemon;
use crate::error::{Error, Result};

/// How often the socket file is checked.
const SOCKET_CHECK: Duration = Duration::from_secs(2);

/// Serve until the daemon is stopped.
pub async fn serve(daemon: Arc<Daemon>) -> Result<()> {
    let socket = daemon.config().dir.socket();
    if socket.exists() {
        if tokio::net::UnixStream::connect(&socket).await.is_ok() {
            return Err(Error::Conflict(format!(
                "a daemon already listens on {}",
                socket.display()
            )));
        }
        std::fs::remove_file(&socket)?;
    }
    let listener = tokio::net::UnixListener::bind(&socket)?;
    restrict(&socket)?;
    let ours = identity(&socket);
    let app = connectrpc::Router::new()
        .add_service(Arc::new(DaemonSvc::new(daemon.clone())))
        .into_axum_router();
    let stop = daemon.shutdown_token();
    let watchdog = {
        let (stop, socket, daemon) = (stop.clone(), socket.clone(), daemon.clone());
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop.cancelled() => return,
                    _ = tokio::time::sleep(SOCKET_CHECK) => {}
                }
                if identity(&socket) != ours {
                    tracing::info!("socket removed or replaced; stopping");
                    daemon.stop();
                    return;
                }
            }
        })
    };
    tracing::info!(socket = %socket.display(), "daemon listening");
    let served = axum::serve(listener, app)
        .with_graceful_shutdown(stop.clone().cancelled_owned())
        .await;
    watchdog.abort();
    daemon.stop();
    if identity(&socket) == ours {
        let _ = std::fs::remove_file(&socket);
    }
    served.map_err(Error::Io)
}

#[cfg(unix)]
fn restrict(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    Ok(std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(0o600),
    )?)
}

/// Which file the socket path names, so a daemon never removes a socket a
/// newer daemon bound.
#[cfg(unix)]
fn identity(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path)
        .ok()
        .map(|m| (m.dev(), m.ino()))
}
