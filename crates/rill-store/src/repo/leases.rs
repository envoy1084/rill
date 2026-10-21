//! Leases: one holder per role and repository, with fencing.
//!
//! A lease is two rows. The record holds the holder and its expiry and is
//! rewritten by every renewal. The epoch row changes only when the lease
//! changes hands, and every write the holder makes checks it
//! ([`Lease::fence`]), so a holder that lost the lease (paused, cut off,
//! or simply slow) cannot write, whatever its clock says.

use bytes::Bytes;
use rill_types::{Record, Timestamp};
use serde::{Deserialize, Serialize};

use super::codec::{decode_record, encode_record, parse_seq};
use super::store::RepoStore;
use crate::error::{Error, Result};
use crate::meta::{Condition, Transaction};

/// How long a lease lasts without renewal.
pub const LEASE_TTL_MS: i64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LeaseRecord {
    holder: String,
    epoch: u64,
    expires_at: Timestamp,
}

impl Record for LeaseRecord {
    const TAG: &'static str = "lease";
}

/// A held lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub role: String,
    pub holder: String,
    pub epoch: u64,
    pub expires_at: Timestamp,
}

impl Lease {
    /// The condition a fenced write carries: the role still has this
    /// holder's epoch.
    pub fn fence(&self, repo: &RepoStore) -> Condition {
        Condition::Equals {
            key: repo.keys().lease_epoch(&self.role),
            value: Some(Bytes::copy_from_slice(&self.epoch.to_be_bytes())),
        }
    }
}

impl RepoStore {
    /// Take `role` for `holder` if it is free, expired or already
    /// `holder`'s. Returns `None` while another holder's lease is live.
    pub async fn try_acquire(&self, role: &str, holder: &str) -> Result<Option<Lease>> {
        let keys = self.keys();
        let (key, epoch_key) = (keys.lease(role), keys.lease_epoch(role));
        let now = Timestamp::now();
        let current = self.meta().get(&key).await?;
        let epoch_row = self.meta().get(&epoch_key).await?;
        let epoch = match &epoch_row {
            Some(bytes) => parse_seq(bytes)?,
            None => 0,
        };
        let record = current
            .as_ref()
            .map(|b| decode_record::<LeaseRecord>(b))
            .transpose()?;
        let keep = match &record {
            Some(r) if r.holder == holder && r.expires_at > now => true,
            Some(r) if r.expires_at > now => return Ok(None),
            _ => false,
        };
        let epoch = if keep { epoch } else { epoch + 1 };
        let next = LeaseRecord {
            holder: holder.to_string(),
            epoch,
            expires_at: Timestamp::from_millis(now.millis() + LEASE_TTL_MS),
        };
        let tx = Transaction::new()
            .expect(key.clone(), current)
            .expect(epoch_key.clone(), epoch_row)
            .set(key, encode_record(&next)?)
            .set(epoch_key, Bytes::copy_from_slice(&epoch.to_be_bytes()));
        match self.meta().transact(tx).await {
            Ok(_) => Ok(Some(Lease {
                role: role.to_string(),
                holder: holder.to_string(),
                epoch,
                expires_at: next.expires_at,
            })),
            Err(Error::Conflict(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Extend `lease`. Fails with a conflict when it was lost.
    pub async fn renew(&self, lease: &Lease) -> Result<Lease> {
        let key = self.keys().lease(&lease.role);
        let current = self.meta().get(&key).await?;
        let held = current
            .as_ref()
            .map(|b| decode_record::<LeaseRecord>(b))
            .transpose()?
            .is_some_and(|r| r.holder == lease.holder && r.epoch == lease.epoch);
        if !held {
            return Err(Error::Conflict(format!("lease {} was lost", lease.role)));
        }
        let next = LeaseRecord {
            holder: lease.holder.clone(),
            epoch: lease.epoch,
            expires_at: Timestamp::from_millis(Timestamp::now().millis() + LEASE_TTL_MS),
        };
        let mut tx = Transaction::new()
            .expect(key.clone(), current)
            .set(key, encode_record(&next)?);
        tx.conditions.push(lease.fence(self));
        self.meta().transact(tx).await?;
        Ok(Lease {
            expires_at: next.expires_at,
            ..lease.clone()
        })
    }

    /// Give up `lease` so a standby can take over at once.
    pub async fn release(&self, lease: &Lease) -> Result<()> {
        let key = self.keys().lease(&lease.role);
        let Some(current) = self.meta().get(&key).await? else {
            return Ok(());
        };
        let record: LeaseRecord = decode_record(&current)?;
        if record.holder != lease.holder || record.epoch != lease.epoch {
            return Ok(());
        }
        let released = LeaseRecord {
            expires_at: Timestamp::from_millis(0),
            ..record
        };
        let tx = Transaction::new()
            .expect(key.clone(), Some(current))
            .set(key, encode_record(&released)?);
        match self.meta().transact(tx).await {
            Ok(_) | Err(Error::Conflict(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// The epoch of `role`'s current holder; 0 when nobody took it.
    pub async fn lease_epoch(&self, role: &str) -> Result<u64> {
        match self.meta().get(&self.keys().lease_epoch(role)).await? {
            Some(bytes) => parse_seq(&bytes),
            None => Ok(0),
        }
    }
}

/// Holds one role's lease for this process: takes it when free, renews
/// it once half has run out, and reports `None` while another holder has
/// it.
pub struct LeaseHolder {
    repo: RepoStore,
    role: String,
    holder: String,
    held: tokio::sync::Mutex<Option<Lease>>,
}

impl LeaseHolder {
    pub fn new(repo: RepoStore, role: &str) -> Self {
        LeaseHolder {
            repo,
            role: role.to_string(),
            holder: holder_id(),
            held: tokio::sync::Mutex::new(None),
        }
    }

    /// The lease, taken or renewed as needed; `None` while another holder
    /// has it.
    pub async fn current(&self) -> Result<Option<Lease>> {
        let mut held = self.held.lock().await;
        let now = Timestamp::now().millis();
        if let Some(lease) = held.as_ref() {
            if lease.expires_at.millis() - now > LEASE_TTL_MS / 2 {
                return Ok(held.clone());
            }
        }
        let renewed = match held.as_ref() {
            Some(lease) => match self.repo.renew(lease).await {
                Ok(lease) => Some(lease),
                Err(Error::Conflict(_)) => {
                    tracing::warn!(role = %self.role, epoch = lease.epoch, "lease lost");
                    None
                }
                Err(e) => return Err(e),
            },
            None => None,
        };
        *held = match renewed {
            Some(lease) => Some(lease),
            None => {
                let taken = self.repo.try_acquire(&self.role, &self.holder).await?;
                if let Some(lease) = &taken {
                    tracing::info!(role = %self.role, epoch = lease.epoch, holder = %lease.holder, "lease taken");
                }
                taken
            }
        };
        Ok(held.clone())
    }

    /// Forget the lease after a fenced write found it lost.
    pub async fn lost(&self) {
        *self.held.lock().await = None;
    }

    /// Give the lease up so a standby can take over at once.
    pub async fn release(&self) {
        if let Some(lease) = self.held.lock().await.take() {
            if let Err(e) = self.repo.release(&lease).await {
                tracing::warn!(role = %self.role, error = %e, "releasing a lease failed");
            }
        }
    }
}

/// A holder name unique to this process.
pub fn holder_id() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        hex::encode(&rill_types::ids::random_id()[..6])
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Stores;

    #[tokio::test]
    async fn one_holder_at_a_time_and_a_deposed_one_is_fenced() {
        let stores = Stores::memory();
        let repo = stores.repo(stores.create_repo("r", "").await.unwrap().id);
        let a = repo.try_acquire("lander", "a").await.unwrap().unwrap();
        assert!(repo.try_acquire("lander", "b").await.unwrap().is_none());
        let a = repo.renew(&a).await.unwrap();
        assert_eq!(
            repo.try_acquire("lander", "a")
                .await
                .unwrap()
                .unwrap()
                .epoch,
            a.epoch
        );

        repo.release(&a).await.unwrap();
        let b = repo.try_acquire("lander", "b").await.unwrap().unwrap();
        assert_eq!(b.epoch, a.epoch + 1);
        assert!(repo.renew(&a).await.unwrap_err().is_conflict());
        let mut write = Transaction::new().set(repo.keys().misc("x"), Bytes::new());
        write.conditions.push(a.fence(&repo));
        assert!(repo.meta().transact(write).await.unwrap_err().is_conflict());
    }
}
