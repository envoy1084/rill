//! The delivery queue: deliveries are jobs, so a failing endpoint is
//! retried with backoff and lands in the dead-letter queue after the last
//! attempt instead of being dropped.

use std::sync::Arc;

use async_trait::async_trait;
use rill_jobs::{Claim, Handler, Job, JobFailure, JobSpec, Queue, Worker};
use rill_types::Principal;
use tokio_util::sync::CancellationToken;

use super::deliver::{Body, Deliverer, Delivery, Target};
use super::message::Message;
use super::prefs::{self, Channel, Topic};
use crate::error::{Error, Result};
use crate::state::RepoContext;

/// The queue deliveries wait in.
pub const NOTIFY_QUEUE: &str = "notify";

fn queue(ctx: &RepoContext) -> Queue {
    Queue::new(ctx.repo.clone(), NOTIFY_QUEUE)
}

/// Queue one delivery.
pub async fn enqueue(ctx: &RepoContext, delivery: &Delivery) -> Result<()> {
    let payload = serde_json::to_vec(delivery).map_err(|e| Error::internal(e.to_string()))?;
    queue(ctx)
        .push(JobSpec::new("deliver", delivery.id.clone(), payload))
        .await?;
    Ok(())
}

fn new_id() -> String {
    format!("msg_{}", hex::encode(rill_types::ids::random_id()))
}

/// The principal an owner entry or display form names, when it names a
/// person (`user:ann`, `ann`, `agent:bot`); teams name nobody here.
fn person(entry: &str) -> Option<Principal> {
    match entry.split_once(':') {
        Some(("user", name)) => Some(Principal::human(name)),
        Some(("agent", name)) => Some(Principal::agent(name)),
        Some(_) => None,
        None => Some(Principal::human(entry)),
    }
}

/// Tell each person among `who` about `message`, on every channel they
/// chose, when they want `topic` (always, for `None`).
pub async fn notify_people(
    ctx: &RepoContext,
    who: &[String],
    topic: Option<Topic>,
    message: &Message,
) -> Result<()> {
    for entry in who {
        let Some(principal) = person(entry) else {
            continue;
        };
        let wanted = prefs::prefs(&ctx.repo, &principal).await?;
        if topic.is_some_and(|t| !wanted.wants(t)) {
            continue;
        }
        for channel in &wanted.channels {
            let target = match channel {
                Channel::Slack { url } => Target::Slack { url: url.clone() },
                Channel::Teams { url } => Target::Teams { url: url.clone() },
                Channel::Email { address } => Target::Email {
                    address: address.clone(),
                },
            };
            enqueue(
                ctx,
                &Delivery {
                    id: new_id(),
                    target,
                    body: Body::Message(message.clone()),
                },
            )
            .await?;
        }
    }
    Ok(())
}

/// A fresh delivery id.
pub fn delivery_id() -> String {
    new_id()
}

struct Sender {
    deliverer: Deliverer,
}

#[async_trait]
impl Handler for Sender {
    async fn run(&self, job: &Job, _claim: &Claim) -> std::result::Result<(), JobFailure> {
        let delivery: Delivery = serde_json::from_slice(&job.payload)
            .map_err(|e| JobFailure::Fatal(format!("bad delivery: {e}")))?;
        self.deliverer.deliver(&delivery).await.map_err(|e| {
            tracing::debug!(id = %delivery.id, error = %e, "delivery failed");
            JobFailure::Retry(e.to_string())
        })
    }
}

/// Deliver queued notifications until `shutdown`.
pub async fn run(ctx: Arc<RepoContext>, shutdown: CancellationToken) {
    let deliverer = Deliverer::new(
        ctx.server_config.notify.clone(),
        ctx.exec.clone(),
        &ctx.config.org,
        &ctx.config.name,
    );
    Worker::new(queue(&ctx), Arc::new(Sender { deliverer }))
        .abort_on(ctx.lifecycle.abort.clone())
        .run(shutdown)
        .await;
}
