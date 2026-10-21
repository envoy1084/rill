//! The queue handle: enqueueing, counters and waiting for work.

use std::time::Duration;

use rill_store::repo::encode_record;
use rill_store::{Mutation, RepoStore, Transaction};
use rill_types::{JobId, Timestamp};

use crate::error::Result;
use crate::job::{Job, JobSpec, Priority};
use crate::retry::RetryPolicy;

/// Shards a queue spreads its ready jobs over, so concurrent claimers
/// mostly touch different key ranges.
pub const DEFAULT_SHARDS: u8 = 4;

/// Queue counter names.
pub mod metric {
    pub const ENQUEUED: &str = "enqueued";
    pub const COMPLETED: &str = "completed";
    pub const RETRIED: &str = "retried";
    pub const DEAD: &str = "dead";
    /// Moves whenever a job becomes ready; idle workers wait on it.
    pub const WAKE: &str = "wake";
}

/// A named queue in one repository.
#[derive(Clone)]
pub struct Queue {
    pub(crate) repo: RepoStore,
    pub(crate) name: String,
    pub(crate) shards: u8,
    pub(crate) policy: RetryPolicy,
}

/// A queue's counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueueStats {
    pub enqueued: i64,
    pub completed: i64,
    pub retried: i64,
    /// Jobs in the dead-letter queue.
    pub dead: i64,
    /// Jobs claimed right now.
    pub leased: usize,
}

impl QueueStats {
    /// Jobs enqueued and not yet finished: ready, waiting to retry or
    /// claimed.
    pub fn pending(&self) -> i64 {
        self.enqueued - self.completed - self.dead
    }
}

impl Queue {
    /// The queue `name` of `repo` with the default policy.
    pub fn new(repo: RepoStore, name: impl Into<String>) -> Self {
        Queue {
            repo,
            name: name.into(),
            shards: DEFAULT_SHARDS,
            policy: RetryPolicy::default(),
        }
    }

    /// The same queue retrying by `policy`.
    pub fn with_policy(mut self, policy: RetryPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn policy(&self) -> &RetryPolicy {
        &self.policy
    }

    /// The mutations enqueueing `spec`, for the transaction that makes
    /// the job necessary.
    pub fn enqueue(&self, spec: JobSpec) -> Result<Vec<Mutation>> {
        let job = Job {
            id: JobId::new(),
            kind: spec.kind,
            key: spec.key,
            payload: spec.payload,
            priority: spec.priority,
            attempt: 0,
            enqueued_at: Timestamp::now(),
            last_error: None,
        };
        let mut mutations = self.ready(&job, job.enqueued_at)?;
        mutations.push(self.count(metric::ENQUEUED, 1));
        Ok(mutations)
    }

    /// Enqueue `spec` on its own.
    pub async fn push(&self, spec: JobSpec) -> Result<()> {
        let mut tx = Transaction::new();
        tx.mutations = self.enqueue(spec)?;
        self.repo.meta().transact(tx).await?;
        Ok(())
    }

    /// The queue's counters.
    pub async fn stats(&self) -> Result<QueueStats> {
        let meta = self.repo.meta();
        let keys = self.repo.keys();
        let read = |m: &str| keys.job_count(&self.name, m);
        Ok(QueueStats {
            enqueued: meta.counter(&read(metric::ENQUEUED)).await?,
            completed: meta.counter(&read(metric::COMPLETED)).await?,
            retried: meta.counter(&read(metric::RETRIED)).await?,
            dead: meta.counter(&read(metric::DEAD)).await?,
            leased: meta
                .scan(rill_store::KeyRange::prefix(keys.jobs_leased(&self.name)))
                .await?
                .len(),
        })
    }

    /// The wake counter now, to pass to [`Queue::wait`].
    pub async fn wake_count(&self) -> Result<i64> {
        Ok(self
            .repo
            .meta()
            .counter(&self.repo.keys().job_count(&self.name, metric::WAKE))
            .await?)
    }

    /// Wait until a job became ready after the wake count was `seen`, or
    /// `timeout` passed.
    pub async fn wait(&self, seen: i64, timeout: Duration) -> Result<()> {
        self.repo
            .meta()
            .wait_counter(
                &self.repo.keys().job_count(&self.name, metric::WAKE),
                seen,
                timeout,
            )
            .await?;
        Ok(())
    }

    /// The mutations making `job` ready from `run_at`.
    pub(crate) fn ready(&self, job: &Job, run_at: Timestamp) -> Result<Vec<Mutation>> {
        let prefix = self.repo.keys().job_ready_at(
            &self.name,
            self.shard_of(&job.key),
            job.priority.rank(),
            run_at.millis(),
        );
        Ok(vec![
            Mutation::AppendStamped {
                prefix,
                value: encode_record(job)?,
            },
            self.count(metric::WAKE, 1),
        ])
    }

    pub(crate) fn count(&self, metric: &str, delta: i64) -> Mutation {
        Mutation::Add {
            key: self.repo.keys().job_count(&self.name, metric),
            delta,
        }
    }

    /// Ready jobs of `shard` at `priority` that may run at `now`, in the
    /// order they became ready.
    pub(crate) fn ready_range(
        &self,
        shard: u8,
        priority: Priority,
        now: Timestamp,
        limit: usize,
    ) -> rill_store::KeyRange {
        let keys = self.repo.keys();
        let rank = priority.rank();
        rill_store::KeyRange::between(
            keys.jobs_ready(&self.name, shard, rank),
            keys.job_ready_at(&self.name, shard, rank, now.millis() + 1),
        )
        .limit(limit)
    }

    /// FNV-1a over the key: stable across processes and releases.
    fn shard_of(&self, key: &str) -> u8 {
        let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
        (hash % u64::from(self.shards.max(1))) as u8
    }
}
