//! The review loop: lease, review, report.

use std::future::Future;
use std::time::Duration;

use rill_sdk::{Lease, ReviewerClient};

use crate::backend::Backend;
use crate::error::Result;

/// How often a held lease is renewed while its job runs (leases last two
/// minutes).
const RENEW_EVERY: Duration = Duration::from_secs(40);

/// What a loop did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    /// Jobs reported.
    pub done: u64,
    /// Jobs given back or refused.
    pub failed: u64,
}

/// Run `work` while renewing `lease`; the lease as last renewed and the
/// work's result.
pub async fn with_renewal<T>(
    reviewers: &ReviewerClient,
    mut lease: Lease,
    work: impl Future<Output = T>,
) -> (Lease, T) {
    tokio::pin!(work);
    let mut tick = tokio::time::interval(RENEW_EVERY);
    tick.tick().await;
    loop {
        tokio::select! {
            out = &mut work => return (lease, out),
            _ = tick.tick() => match reviewers.renew(&lease).await {
                Ok(renewed) => lease = renewed,
                Err(e) => tracing::warn!(error = %e, "renewing a lease failed"),
            },
        }
    }
}

/// Review jobs until none is waiting (`once`) or forever.
pub async fn run(
    reviewers: &ReviewerClient,
    backend: &dyn Backend,
    wait_ms: u32,
    once: bool,
) -> Result<Tally> {
    let mut tally = Tally::default();
    loop {
        let Some((lease, job)) = reviewers.lease_review(wait_ms).await? else {
            if once {
                return Ok(tally);
            }
            continue;
        };
        tracing::info!(change = %job.change, version = %job.version, reason = %job.reason, "reviewing");
        let (lease, report) = with_renewal(reviewers, lease, backend.review(&job)).await;
        match report {
            Ok(report) => match reviewers.complete_review(&lease, &report).await {
                Ok(run) => {
                    tracing::info!(change = %job.change, vote = run.vote, findings = run.findings, "review posted");
                    tally.done += 1;
                }
                Err(e) => {
                    tracing::warn!(change = %job.change, error = %e, "the server refused the review");
                    tally.failed += 1;
                }
            },
            Err(e) => {
                tracing::warn!(change = %job.change, error = %e, "the backend failed; the job will be retried");
                reviewers.fail(&lease, &e.to_string(), true).await?;
                tally.failed += 1;
            }
        }
    }
}
