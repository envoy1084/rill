//! The background push loop of a mount: wait for edits, let them settle
//! for the sync delay, push, and back off while the server is unreachable.

use std::sync::Arc;
use std::time::Duration;

use crate::mount::Mount;

const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Start pushing `mount`'s edits until it is cancelled.
pub(crate) fn spawn(mount: Arc<Mount>) {
    tokio::spawn(async move {
        let delay = mount.shared.config.sync_delay;
        loop {
            tokio::select! {
                _ = mount.cancel.cancelled() => return,
                _ = mount.dirty.notified() => {}
            }
            tokio::select! {
                _ = mount.cancel.cancelled() => return,
                _ = tokio::time::sleep(delay) => {}
            }
            let mut backoff = delay.max(Duration::from_millis(500));
            loop {
                match mount.push().await {
                    Ok(_) => {
                        *mount.sync_error.lock() = None;
                        break;
                    }
                    Err(e) => {
                        tracing::warn!(mount = %mount.id(), error = %e, "push failed; will retry");
                        *mount.sync_error.lock() = Some(e.to_string());
                        tokio::select! {
                            _ = mount.cancel.cancelled() => return,
                            _ = tokio::time::sleep(backoff) => {}
                        }
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                    }
                }
            }
        }
    });
}
