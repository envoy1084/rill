//! Running a role only while this process holds the role's lease, so one
//! server in the cluster does it and a standby takes over when that server
//! dies or is partitioned away.

use std::future::Future;
use std::time::Duration;

use rill_store::repo::{Lease, LeaseHolder, LEASE_TTL_MS};
use rill_store::RepoStore;
use tokio_util::sync::CancellationToken;

/// How often a standby tries for the lease.
const STANDBY_POLL: Duration = Duration::from_millis(LEASE_TTL_MS as u64 / 2);
/// How often the holder renews; well inside the lease.
const RENEW_EVERY: Duration = Duration::from_millis(LEASE_TTL_MS as u64 / 4);

/// Run `work` each time this process takes `role`'s lease, with the lease
/// and a token cancelled when the lease is lost or `shutdown` fires.
/// Returns after `shutdown`, giving the lease up.
pub async fn while_leader<F, Fut>(
    repo: RepoStore,
    role: &str,
    shutdown: CancellationToken,
    mut work: F,
) where
    F: FnMut(Lease, CancellationToken) -> Fut,
    Fut: Future<Output = ()>,
{
    let holder = LeaseHolder::new(repo, role);
    loop {
        let lease = loop {
            match holder.current().await {
                Ok(Some(lease)) => break lease,
                Ok(None) => {}
                Err(e) => tracing::warn!(role, error = %e, "taking a lease failed"),
            }
            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = tokio::time::sleep(STANDBY_POLL) => {}
            }
        };
        tracing::info!(role, epoch = lease.epoch, "leading");
        let term = shutdown.child_token();
        let keep = async {
            loop {
                tokio::time::sleep(RENEW_EVERY).await;
                match holder.current().await {
                    Ok(Some(current)) if current.epoch == lease.epoch => {}
                    Ok(_) => break,
                    Err(e) => tracing::warn!(role, error = %e, "renewing a lease failed"),
                }
            }
            tracing::warn!(role, epoch = lease.epoch, "lease lost; standing by");
            term.cancel();
        };
        tokio::select! {
            _ = work(lease.clone(), term.clone()) => {}
            _ = keep => {}
        }
        term.cancel();
        if shutdown.is_cancelled() {
            holder.release().await;
            return;
        }
    }
}
