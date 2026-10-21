//! Revert jobs: automatic reverts the post-land verifier scheduled, run
//! once their grace period is over.

use std::sync::Arc;

use rill_jobs::{Queue, Worker};
use rill_lander::postland::{RevertHandler, REVERT_QUEUE};
use tokio_util::sync::CancellationToken;

use crate::state::RepoContext;

/// Run revert jobs until `shutdown`.
pub async fn run(ctx: Arc<RepoContext>, shutdown: CancellationToken) {
    let queue = Queue::new(ctx.repo.clone(), REVERT_QUEUE);
    Worker::new(queue, RevertHandler::new(ctx.lander.clone()))
        .abort_on(ctx.lifecycle.abort.clone())
        .run(shutdown)
        .await;
}
