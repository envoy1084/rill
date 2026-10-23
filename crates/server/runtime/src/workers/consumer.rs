//! Durable consumers of the event log. One server runs each consumer (it
//! holds the consumer's lease); it resumes after the last event it
//! handled, whose cursor it saves under the lease, so a restart or a
//! failover neither skips nor (beyond the event in flight) repeats events.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use rill_events::{EventFilter, Start};
use rill_types::{Event, Versionstamp};
use tokio_util::sync::CancellationToken;

use super::leader::while_leader;
use crate::error::Result;
use crate::state::RepoContext;

/// Attempts at one event before it is skipped with an error logged.
const ATTEMPTS: u32 = 5;
/// Pause between attempts.
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Something that reacts to events.
#[async_trait]
pub trait Consumer: Send + Sync + 'static {
    /// Stable name: the lease and the saved cursor are keyed by it.
    fn name(&self) -> &'static str;
    /// The events it wants.
    fn filter(&self) -> EventFilter;
    /// React to one event.
    async fn handle(&self, ctx: &Arc<RepoContext>, event: &Event) -> Result<()>;
}

fn cursor_key(name: &str) -> String {
    format!("consumer:{name}")
}

/// Run `consumer` over `ctx`'s event log until `shutdown`. On its first
/// run it starts at the newest event.
pub async fn run(ctx: Arc<RepoContext>, consumer: Arc<dyn Consumer>, shutdown: CancellationToken) {
    let role = format!("consumer-{}", consumer.name());
    while_leader(ctx.repo.clone(), &role, shutdown, |lease, term| {
        let (ctx, consumer) = (ctx.clone(), consumer.clone());
        async move {
            if let Err(e) = consume(&ctx, consumer.as_ref(), &lease, term).await {
                tracing::warn!(repo = %ctx.config.name, consumer = consumer.name(), error = %e, "event consumer stopped");
            }
        }
    })
    .await;
}

async fn consume(
    ctx: &Arc<RepoContext>,
    consumer: &dyn Consumer,
    lease: &rill_store::repo::Lease,
    stop: CancellationToken,
) -> Result<()> {
    let key = cursor_key(consumer.name());
    let mut saved = ctx.repo.get_misc(&key).await?;
    let start = match saved.as_deref().map(Versionstamp::from_slice) {
        Some(Ok(stamp)) => Start::After(stamp),
        _ => Start::Now,
    };
    let log = rill_events::tail(ctx.repo.clone(), start, consumer.filter()).await?;
    let mut log = Box::pin(log);
    loop {
        let next = tokio::select! {
            _ = stop.cancelled() => return Ok(()),
            next = log.next() => next,
        };
        let Some(next) = next else { return Ok(()) };
        let (stamp, event) = next?;
        for attempt in 1..=ATTEMPTS {
            match consumer.handle(ctx, &event).await {
                Ok(()) => break,
                Err(e) if attempt == ATTEMPTS => tracing::error!(
                    repo = %ctx.config.name, consumer = consumer.name(), event = event.kind.name(), error = %e,
                    "skipping an event after repeated failures"
                ),
                Err(e) => {
                    tracing::debug!(consumer = consumer.name(), error = %e, "retrying an event");
                    tokio::time::sleep(RETRY_DELAY).await;
                }
            }
        }
        let value = Bytes::copy_from_slice(&stamp.0);
        ctx.repo
            .set_misc_if(&key, saved.take(), value.clone(), lease)
            .await?;
        saved = Some(value);
    }
}
