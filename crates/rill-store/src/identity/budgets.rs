//! What agents spend, counted exactly: a budget check and its charge
//! commit together, so a budget is never exceeded however many requests
//! race.

use bytes::Bytes;
use rill_types::Principal;

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::Transaction;

fn key(principal: &Principal, metric: &str, day: i64) -> Bytes {
    keys::identity_row(table::IDENT_BUDGET, (principal.id.to_string(), metric, day))
}

fn decode(bytes: &[u8]) -> Result<u64> {
    let raw: [u8; 8] = bytes
        .try_into()
        .map_err(|_| Error::Codec("budget counter is not 8 bytes".into()))?;
    Ok(u64::from_be_bytes(raw))
}

impl IdentityStore {
    /// What `principal` spent of `metric` on `day`.
    pub async fn spent(&self, principal: &Principal, metric: &str, day: i64) -> Result<u64> {
        match self.meta.get(&key(principal, metric, day)).await? {
            Some(bytes) => decode(&bytes),
            None => Ok(0),
        }
    }

    /// The writes that spend one unit of `metric` on `day`, conditional on
    /// nothing else spending meanwhile; `None` when `limit` is reached. Add
    /// them to the transaction doing the work, and retry it on conflict.
    pub async fn spend(
        &self,
        principal: &Principal,
        metric: &str,
        day: i64,
        limit: u64,
    ) -> Result<Option<Transaction>> {
        let key = key(principal, metric, day);
        let current = self.meta.get(&key).await?;
        let spent = match &current {
            Some(bytes) => decode(bytes)?,
            None => 0,
        };
        if spent >= limit {
            return Ok(None);
        }
        let next = Bytes::copy_from_slice(&(spent + 1).to_be_bytes());
        Ok(Some(
            Transaction::new()
                .expect(key.clone(), current)
                .set(key, next),
        ))
    }
}
