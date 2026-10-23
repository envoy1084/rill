//! The macOS front end of the view: an NFSv3 server on 127.0.0.1 (one per
//! mount, through `nfsserve`) that macOS's built-in NFS client mounts. No
//! kernel extension and no privileges are needed. Writes are buffered per
//! file and committed when the file goes quiet (`writes.rs`).

mod fs;
mod mounting;
mod writes;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nfsserve::tcp::{NFSTcp, NFSTcpListener};

use crate::error::Result;
use crate::mount::Mount;
use crate::vfs::{Session, View};
use writes::{Writes, QUIET};

/// How often quiet files are committed.
const SWEEP: Duration = Duration::from_millis(200);

/// Keeps the export mounted and served; unmounts on drop.
struct NfsSession {
    path: PathBuf,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for NfsSession {
    fn drop(&mut self) {
        mounting::unmount(&self.path);
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// Serve `view` and mount it at `mount`'s directory.
pub(crate) async fn start(mount: &Arc<Mount>, view: Arc<View>) -> Result<Session> {
    let path = mount.record().path;
    mounting::clear_stale(&path);
    let writes = Arc::new(Writes::default());
    let fsid = u64::from_le_bytes(
        crate::cache::local_digest(mount.id().as_bytes()).as_bytes()[..8]
            .try_into()
            .unwrap_or_default(),
    );
    let listener = NFSTcpListener::bind(
        "127.0.0.1:0",
        fs::NfsView::new(view.clone(), writes.clone(), fsid),
    )
    .await?;
    let port = listener.get_listen_port();
    let server = tokio::spawn(async move {
        if let Err(e) = listener.handle_forever().await {
            tracing::warn!(error = %e, "NFS server stopped");
        }
    });
    let sweeper = {
        let (view, writes, cancel) = (view.clone(), writes.clone(), mount.cancel.clone());
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(SWEEP) => {}
                }
                if let Err(e) = writes.commit(&view, QUIET).await {
                    tracing::warn!(error = %e, "committing writes failed");
                }
            }
        })
    };
    let settle_writes = writes.clone();
    let settle_view = view.clone();
    let _ = mount.settler.set(Arc::new(move || {
        let (writes, view) = (settle_writes.clone(), settle_view.clone());
        Box::pin(async move { writes.commit(&view, Duration::ZERO).await })
    }));
    let session = NfsSession {
        path: path.clone(),
        tasks: vec![server, sweeper],
    };
    mounting::mount(port, &path).await?;
    tracing::info!(path = %path.display(), port, "view mounted over NFS");
    Ok(Box::new(session))
}
