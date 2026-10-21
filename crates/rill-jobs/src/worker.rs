//! A runtime that drives a handler over a queue: claim, run while
//! renewing the claim, then complete or fail; plus a reaper that takes
//! back the jobs of workers that died.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::claim::Claim;
use crate::error::{Error, Result};
use crate::job::Job;
use crate::queue::Queue;

/// How long an idle worker waits for new work before looking again;
/// retries whose backoff ran out do not wake it.
const IDLE_WAIT: Duration = Duration::from_secs(1);

/// Why a job's attempt failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JobFailure {
    /// Worth retrying.
    #[error("{0}")]
    Retry(String),
    /// Retrying cannot help; the job goes to the dead-letter queue.
    #[error("{0}")]
    Fatal(String),
}

/// What runs a queue's jobs. A job may run more than once (a worker can
/// die after doing the work and before completing), so handlers must be
/// idempotent or fence their writes with [`Claim::fence`].
#[async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn run(&self, job: &Job, claim: &Claim) -> std::result::Result<(), JobFailure>;
}

/// Workers for one queue.
pub struct Worker<H> {
    queue: Queue,
    handler: Arc<H>,
    holder: String,
    concurrency: usize,
    abort: CancellationToken,
}

impl<H: Handler> Worker<H> {
    /// One worker over `queue`.
    pub fn new(queue: Queue, handler: Arc<H>) -> Self {
        Worker {
            queue,
            handler,
            holder: rill_store::repo::holder_id(),
            concurrency: 1,
            abort: CancellationToken::new(),
        }
    }

    /// Give running jobs back to the queue when `abort` fires. `run`'s
    /// shutdown only stops new claims; jobs already running finish unless
    /// this fires first.
    pub fn abort_on(mut self, abort: CancellationToken) -> Self {
        self.abort = abort;
        self
    }

    /// Run `n` jobs at a time.
    pub fn concurrency(mut self, n: usize) -> Self {
        self.concurrency = n.max(1);
        self
    }

    /// Process jobs until `shutdown`.
    pub async fn run(self, shutdown: CancellationToken) {
        let this = Arc::new(self);
        let mut tasks = vec![tokio::spawn(this.clone().reap_forever(shutdown.clone()))];
        for _ in 0..this.concurrency {
            tasks.push(tokio::spawn(this.clone().work_forever(shutdown.clone())));
        }
        for task in tasks {
            let _ = task.await;
        }
    }

    /// Claim and run one job; `false` when none was ready.
    pub async fn run_once(&self) -> Result<bool> {
        let Some(claim) = self.queue.claim(&self.holder).await? else {
            return Ok(false);
        };
        self.process(claim).await?;
        Ok(true)
    }

    async fn work_forever(self: Arc<Self>, shutdown: CancellationToken) {
        while !shutdown.is_cancelled() {
            let seen = match self.queue.wake_count().await {
                Ok(seen) => seen,
                Err(e) => {
                    tracing::warn!(queue = %self.queue.name(), error = %e, "reading the queue failed");
                    0
                }
            };
            match self.run_once().await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(queue = %self.queue.name(), error = %e, "job attempt failed")
                }
            }
            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = self.queue.wait(seen, IDLE_WAIT) => {}
            }
        }
    }

    async fn reap_forever(self: Arc<Self>, shutdown: CancellationToken) {
        let mut tick = tokio::time::interval(self.queue.policy().lease / 2);
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = tick.tick() => {}
            }
            if let Err(e) = self.queue.reap().await {
                tracing::warn!(queue = %self.queue.name(), error = %e, "reaping expired claims failed");
            }
        }
    }

    /// Run `claim`'s job while renewing the claim; stop if it is lost.
    async fn process(&self, claim: Claim) -> Result<()> {
        let outcome = tokio::select! {
            outcome = self.handler.run(&claim.job, &claim) => outcome,
            lost = self.keep(claim.clone()) => return Err(lost),
            _ = self.abort.cancelled() => {
                tracing::info!(queue = %self.queue.name(), job = %claim.job.id, "giving an unfinished job back");
                return self.queue.release(&claim).await;
            }
        };
        match outcome {
            Ok(()) => self.queue.complete(&claim).await,
            Err(failure) => {
                let permanent = matches!(failure, JobFailure::Fatal(_));
                tracing::debug!(queue = %self.queue.name(), job = %claim.job.id, error = %failure, "job attempt failed");
                self.queue
                    .fail(&claim, &failure.to_string(), permanent)
                    .await
            }
        }
    }

    /// Renew `claim` until it is lost; returns the loss.
    async fn keep(&self, mut claim: Claim) -> Error {
        let every = self.queue.policy().lease / 3;
        loop {
            tokio::time::sleep(every).await;
            match self.queue.renew(&claim).await {
                Ok(renewed) => claim = renewed,
                Err(lost @ Error::Lost(_)) => return lost,
                Err(e) => {
                    tracing::warn!(queue = %self.queue.name(), error = %e, "renewing a claim failed")
                }
            }
        }
    }
}
