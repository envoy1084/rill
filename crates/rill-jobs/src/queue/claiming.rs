//! Claiming a job, keeping the claim, and finishing it.

use bytes::Bytes;
use rill_store::repo::{decode_record, encode_record};
use rill_store::{Error as StoreError, Transaction};
use rill_types::{ids::random_id, Timestamp};

use super::handle::{metric, Queue};
use crate::claim::Claim;
use crate::error::{Error, Result};
use crate::job::{Job, Leased, Priority};

/// Ready jobs read per shard and priority while claiming.
const CLAIM_SCAN: usize = 8;

impl Queue {
    /// Claim the most urgent job that may run now, for `holder`. Shards
    /// are tried from a random one so claimers spread out.
    pub async fn claim(&self, holder: &str) -> Result<Option<Claim>> {
        let now = Timestamp::now();
        let shards = self.shards.max(1);
        let first = random_id()[0] % shards;
        for priority in Priority::ALL {
            for i in 0..shards {
                let shard = (first + i) % shards;
                let range = self.ready_range(shard, priority, now, CLAIM_SCAN);
                for (key, value) in self.repo.meta().scan(range).await? {
                    let job: Job = decode_record(&value)?;
                    let (tx, claim) = self.claim_tx(job, holder, now)?;
                    let tx = tx.expect(key.clone(), Some(value)).delete(key);
                    match self.repo.meta().transact(tx).await {
                        Ok(_) => return Ok(Some(claim)),
                        Err(StoreError::Conflict(_)) => continue,
                        Err(e) => return Err(e.into()),
                    }
                }
            }
        }
        Ok(None)
    }

    /// Extend `claim`'s lease; fails with [`Error::Lost`] once the job was
    /// taken back.
    pub async fn renew(&self, claim: &Claim) -> Result<Claim> {
        let lease_until = self.lease_from(Timestamp::now());
        let leased = Leased {
            job: claim.job.clone(),
            holder: claim.holder.clone(),
            token: claim.token,
            lease_until,
        };
        let value = encode_record(&leased)?;
        let mut tx = Transaction::new()
            .expect(claim.leased_key.clone(), Some(claim.leased_value.clone()))
            .set(claim.leased_key.clone(), value.clone());
        tx.conditions.push(claim.fence());
        self.commit_claimed(claim, tx).await?;
        Ok(Claim {
            lease_until,
            leased_value: value,
            ..claim.clone()
        })
    }

    /// Finish `claim`'s job.
    pub async fn complete(&self, claim: &Claim) -> Result<()> {
        self.commit_claimed(claim, self.complete_tx(claim)).await
    }

    /// Give `claim`'s job back unfinished, ready at once and without
    /// counting the attempt: a worker shutting down hands its jobs on
    /// instead of making them wait for the claim to expire.
    pub async fn release(&self, claim: &Claim) -> Result<()> {
        let mut tx = Transaction::new()
            .delete(claim.leased_key.clone())
            .delete(claim.token_key.clone());
        tx.mutations
            .extend(self.ready(&claim.job, Timestamp::now())?);
        tx.conditions.push(claim.fence());
        self.commit_claimed(claim, tx).await
    }

    /// The transaction finishing `claim`'s job, to commit together with
    /// the job's own writes.
    pub fn complete_tx(&self, claim: &Claim) -> Transaction {
        let mut tx = Transaction::new()
            .delete(claim.leased_key.clone())
            .delete(claim.token_key.clone());
        tx.conditions.push(claim.fence());
        tx.mutations.push(self.count(metric::COMPLETED, 1));
        tx
    }

    /// Commit `tx`, fenced on `claim`; a conflict means the claim was
    /// lost.
    pub(crate) async fn commit_claimed(&self, claim: &Claim, tx: Transaction) -> Result<()> {
        match self.repo.meta().transact(tx).await {
            Ok(_) => Ok(()),
            Err(StoreError::Conflict(_)) => Err(Error::Lost(claim.job.id)),
            Err(e) => Err(e.into()),
        }
    }

    fn claim_tx(&self, job: Job, holder: &str, now: Timestamp) -> Result<(Transaction, Claim)> {
        let keys = self.repo.keys();
        let id = *job.id.as_bytes();
        let leased = Leased {
            job,
            holder: holder.to_string(),
            token: random_id(),
            lease_until: self.lease_from(now),
        };
        let leased_key = keys.job_leased(&self.name, &id);
        let token_key = keys.job_token(&self.name, &id);
        let leased_value = encode_record(&leased)?;
        let tx = Transaction::new()
            .set(leased_key.clone(), leased_value.clone())
            .set(token_key.clone(), Bytes::copy_from_slice(&leased.token));
        let claim = Claim {
            job: leased.job,
            holder: leased.holder,
            lease_until: leased.lease_until,
            token: leased.token,
            token_key,
            leased_key,
            leased_value,
        };
        Ok((tx, claim))
    }

    fn lease_from(&self, now: Timestamp) -> Timestamp {
        Timestamp::from_millis(now.millis() + self.policy.lease.as_millis() as i64)
    }
}
