//! Failed attempts: retrying with backoff, taking back expired claims,
//! and the dead-letter queue.

use rill_store::repo::{decode_record, encode_record};
use rill_store::{Error as StoreError, KeyRange, Transaction};
use rill_types::{JobId, Timestamp};

use super::handle::{metric, Queue};
use crate::claim::Claim;
use crate::error::Result;
use crate::job::{DeadJob, Job, Leased};

impl Queue {
    /// Record that `claim`'s attempt failed with `error`: the job is
    /// retried after a backoff, or goes to the dead-letter queue when it
    /// is out of attempts or `permanent`.
    pub async fn fail(&self, claim: &Claim, error: &str, permanent: bool) -> Result<()> {
        let mut tx = self
            .after_failure(claim.job.clone(), error, permanent)?
            .delete(claim.leased_key.clone())
            .delete(claim.token_key.clone());
        tx.conditions.push(claim.fence());
        self.commit_claimed(claim, tx).await
    }

    /// Take back every job whose claim expired, as a failed attempt;
    /// returns how many.
    pub async fn reap(&self) -> Result<usize> {
        let now = Timestamp::now();
        let keys = self.repo.keys();
        let rows = self
            .repo
            .meta()
            .scan(KeyRange::prefix(keys.jobs_leased(&self.name)))
            .await?;
        let mut reaped = 0;
        for (key, value) in rows {
            let leased: Leased = decode_record(&value)?;
            if leased.lease_until > now {
                continue;
            }
            let id = *leased.job.id.as_bytes();
            tracing::info!(queue = %self.name, job = %leased.job.id, holder = %leased.holder, "claim expired; taking the job back");
            let tx = self
                .after_failure(leased.job, "claim expired", false)?
                .expect(key.clone(), Some(value))
                .delete(key)
                .delete(keys.job_token(&self.name, &id));
            match self.repo.meta().transact(tx).await {
                Ok(_) => reaped += 1,
                // Renewed or finished since the scan.
                Err(StoreError::Conflict(_)) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(reaped)
    }

    /// The oldest `limit` jobs in the dead-letter queue.
    pub async fn dead(&self, limit: usize) -> Result<Vec<DeadJob>> {
        let range = KeyRange::prefix(self.repo.keys().jobs_dead(&self.name)).limit(limit);
        self.repo
            .meta()
            .scan(range)
            .await?
            .iter()
            .map(|(_, v)| Ok(decode_record(v)?))
            .collect()
    }

    /// Move a dead job back to the queue with fresh attempts; `false` if
    /// it is not in the dead-letter queue.
    pub async fn retry_dead(&self, id: &JobId) -> Result<bool> {
        let key = self.repo.keys().job_dead(&self.name, id.as_bytes());
        let Some(value) = self.repo.meta().get(&key).await? else {
            return Ok(false);
        };
        let dead: DeadJob = decode_record(&value)?;
        let job = Job {
            attempt: 0,
            ..dead.job
        };
        let mut tx = Transaction::new()
            .expect(key.clone(), Some(value))
            .delete(key);
        tx.mutations.extend(self.ready(&job, Timestamp::now())?);
        tx.mutations.push(self.count(metric::DEAD, -1));
        match self.repo.meta().transact(tx).await {
            Ok(_) => Ok(true),
            Err(StoreError::Conflict(_)) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    fn after_failure(&self, mut job: Job, error: &str, permanent: bool) -> Result<Transaction> {
        let now = Timestamp::now();
        job.last_error = Some(error.to_string());
        let mut tx = Transaction::new();
        if !permanent && job.attempt + 1 < self.policy.max_attempts {
            job.attempt += 1;
            let delay = self.policy.delay(job.attempt).as_millis() as i64;
            tx.mutations
                .extend(self.ready(&job, Timestamp::from_millis(now.millis() + delay))?);
            tx.mutations.push(self.count(metric::RETRIED, 1));
        } else {
            tracing::warn!(queue = %self.name, job = %job.id, kind = %job.kind, error, "job is dead");
            let key = self.repo.keys().job_dead(&self.name, job.id.as_bytes());
            let dead = DeadJob {
                job,
                error: error.to_string(),
                died_at: now,
            };
            tx = tx.set(key, encode_record(&dead)?);
            tx.mutations.push(self.count(metric::DEAD, 1));
        }
        Ok(tx)
    }
}
