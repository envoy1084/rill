//! Building the audit log: project the identity log and every
//! repository's event log into the queue, chain each org's queued records,
//! and seal chains that are due. Every server may run it: cursors and
//! chain heads move by compare-and-set, so a second worker only loses
//! races.

use std::sync::Arc;
use std::time::Duration;

use rill_store::audit::CHAIN_BATCH;
use rill_types::audit::INSTANCE_AUDIT;
use tokio_util::sync::CancellationToken;

use super::{project, seal};
use crate::error::Result;
use crate::state::AppState;

/// How long the worker waits for more work between passes.
const PASS: Duration = Duration::from_millis(500);
/// Log entries projected per transaction.
const PROJECT_BATCH: usize = 500;

/// Run until `shutdown`.
pub async fn run(state: Arc<AppState>, shutdown: CancellationToken) {
    loop {
        if let Err(e) = pass(&state).await {
            tracing::warn!(error = %e, "audit pass failed");
        }
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(PASS) => {}
        }
    }
}

/// Project and chain everything outstanding.
pub async fn catch_up(state: &AppState) -> Result<()> {
    project_identity(state).await?;
    project_repos(state).await?;
    let audit = state.stores.audit();
    for org in audit.pending_orgs().await? {
        while audit.chain(&org).await? == CHAIN_BATCH {}
    }
    Ok(())
}

/// One pass: catch up, then seal the chains that are due.
pub async fn pass(state: &AppState) -> Result<()> {
    catch_up(state).await?;
    let every_ms = (state.config.audit.seal_every_secs as i64).saturating_mul(1000);
    let mut orgs: Vec<String> = state
        .stores
        .list_orgs()
        .await?
        .into_iter()
        .map(|o| o.name)
        .collect();
    orgs.push(INSTANCE_AUDIT.to_string());
    for org in orgs {
        if seal::due(state, &org, every_ms).await? {
            seal::seal(state, &org).await?;
        }
    }
    Ok(())
}

async fn project_identity(state: &AppState) -> Result<()> {
    let audit = state.stores.audit();
    let identity = state.stores.identity();
    loop {
        let (cursor, raw) = audit.cursor("identity").await?;
        let entries = identity.log_after(cursor, PROJECT_BATCH).await?;
        let Some((last, _)) = entries.last() else {
            return Ok(());
        };
        let records: Vec<_> = entries.iter().map(|(_, e)| project::identity(e)).collect();
        match audit.project("identity", raw, *last, &records).await {
            Ok(()) if entries.len() == PROJECT_BATCH => continue,
            Ok(()) => return Ok(()),
            Err(e) if e.is_conflict() => return Ok(()),
            Err(e) => return Err(e.into()),
        }
    }
}

async fn project_repos(state: &AppState) -> Result<()> {
    let audit = state.stores.audit();
    for config in state.repos.list().await? {
        let repo = state.stores.org(&config.org).repo(config.id);
        let source = format!("repo:{}", config.id);
        loop {
            let (cursor, raw) = audit.cursor(&source).await?;
            let events = repo.events_after(cursor, PROJECT_BATCH).await?;
            let Some((last, _)) = events.last() else {
                break;
            };
            let records: Vec<_> = events
                .iter()
                .filter_map(|(_, e)| project::repo(&config.org, &config.name, e))
                .collect();
            match audit.project(&source, raw, *last, &records).await {
                Ok(()) if events.len() == PROJECT_BATCH => continue,
                Ok(()) => break,
                Err(e) if e.is_conflict() => break,
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(())
}
