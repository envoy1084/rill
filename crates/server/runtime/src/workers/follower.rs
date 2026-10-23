//! Keeping this server's cached view of shared state current: other
//! servers change policy and relations too, and the event log says when.

use std::sync::Arc;

use futures::StreamExt;
use rill_events::{EventFilter, Start};
use rill_types::EventKind;
use tokio_util::sync::CancellationToken;

use crate::state::RepoContext;

/// Reload the review policy on every `PolicyChanged` and every landing that
/// touches `.rill/policy/`, and drop the cached relation snapshot on every
/// `RelationChanged`, whichever server wrote them, until `shutdown`.
pub async fn run(ctx: Arc<RepoContext>, shutdown: CancellationToken) {
    let filter = EventFilter::kinds(["policy_changed", "relation_changed", "landed"]);
    let log = match rill_events::tail(ctx.repo.clone(), Start::Now, filter).await {
        Ok(log) => log,
        Err(e) => {
            tracing::error!(repo = %ctx.config.name, error = %e, "following shared state failed");
            return;
        }
    };
    let mut log = Box::pin(log);
    loop {
        let next = tokio::select! {
            _ = shutdown.cancelled() => return,
            next = log.next() => next,
        };
        match next {
            Some(Ok((_, event))) => match event.kind {
                EventKind::PolicyChanged { name } => {
                    if let Err(e) = ctx.reload_policy().await {
                        tracing::warn!(policy = %name, error = %e, "reloading policy failed");
                    }
                }
                EventKind::RelationChanged { .. } => ctx.authorizer.invalidate(),
                EventKind::Landed { version, .. } => {
                    let touches = match ctx.derive.change(&version).await {
                        Ok(cv) => cv
                            .file_changes
                            .keys()
                            .any(|p| p.as_str().starts_with(crate::state::policy::POLICY_DIR)),
                        Err(_) => false,
                    };
                    if touches {
                        if let Err(e) = ctx.reload_policy().await {
                            tracing::warn!(error = %e, "reloading policy after a landing failed");
                        }
                    }
                }
                _ => {}
            },
            Some(Err(e)) => {
                tracing::warn!(error = %e, "reading the event log failed");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            None => return,
        }
    }
}
