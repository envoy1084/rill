//! The cache's routes and serving them with the origin followers running.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use tokio_util::sync::CancellationToken;

use super::metrics::metrics;
use super::objects::{get_object, head_object};
use super::Cache;
use crate::config::CacheConfig;
use crate::error::{Error, Result};

/// The cache's routes.
pub fn router(cache: Arc<Cache>) -> Router {
    Router::new()
        .route("/o/{repo}/{kind}/{hash}", get(get_object).head(head_object))
        .route("/metrics", get(metrics))
        .route("/livez", get(|| async { "ok" }))
        .with_state(cache)
}

/// Serve `cache` on `listener` until `shutdown`, rereading the origin's
/// grant keys and redactions as `config` says.
pub async fn serve_on(
    cache: Arc<Cache>,
    listener: tokio::net::TcpListener,
    config: &CacheConfig,
    shutdown: CancellationToken,
) -> Result<()> {
    let (key_refresh, redaction_poll) = (config.key_refresh, config.redaction_poll);
    let follower = cache.clone();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop.cancelled() => return,
                _ = tokio::time::sleep(redaction_poll) => {}
            }
            match follower.redactions.refresh().await {
                Ok(new) => {
                    for hash in new {
                        if let Err(e) = follower.disk.remove(&hash).await {
                            tracing::warn!(error = %e, %hash, "dropping a redacted object failed");
                        }
                    }
                }
                Err(e) => tracing::warn!(error = %e, "rereading redactions failed"),
            }
        }
    });
    let keys = cache.keys.clone();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop.cancelled() => return,
                _ = tokio::time::sleep(key_refresh) => {}
            }
            if let Err(e) = keys.refresh().await {
                tracing::warn!(error = %e, "rereading grant keys failed");
            }
        }
    });
    axum::serve(listener, router(cache))
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .map_err(Error::Io)
}

/// Run a cache from `config` until `shutdown`.
pub async fn serve(config: CacheConfig, shutdown: CancellationToken) -> Result<()> {
    let cache = Cache::start(&config).await?;
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let addr: SocketAddr = listener.local_addr()?;
    tracing::info!(%addr, origin = %config.origin, dir = %config.dir.display(), max_bytes = config.max_bytes, "cache serving");
    serve_on(cache, listener, &config, shutdown).await
}
