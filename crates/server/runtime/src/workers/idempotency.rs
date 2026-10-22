//! Sweeping expired idempotency records.

/// Delete expired idempotency records every ten minutes until `shutdown`.
pub async fn sweep_idempotency(
    stores: rill_store::Stores,
    shutdown: tokio_util::sync::CancellationToken,
) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(600));
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tick.tick() => {}
        }
        match stores.idem_sweep(10_000).await {
            Ok(0) => {}
            Ok(n) => tracing::debug!(removed = n, "swept expired idempotency records"),
            Err(e) => tracing::warn!(error = %e, "sweeping idempotency records failed"),
        }
    }
}
