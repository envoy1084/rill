//! Retry safety for mutations: one stored outcome per (principal,
//! operation id).
//!
//! A request first claims its operation id. A retry of a finished request
//! gets the stored response; a duplicate of one still running is told so;
//! reusing an id for a different request is refused. A failed request
//! releases its claim so the client can retry. Claims and responses expire
//! after [`IDEM_TTL_MS`]; a claim held past [`CLAIM_TTL_MS`] by a server
//! that died can be taken over.

use bytes::Bytes;
use rill_types::{Record, Timestamp};
use serde::{Deserialize, Serialize};

use super::codec::{decode_record, encode_record};
use super::stores::Stores;
use crate::error::{Error, Result};
use crate::keys;
use crate::meta::{KeyRange, Mutation, Transaction};

/// How long a finished request's response is kept.
pub const IDEM_TTL_MS: i64 = 24 * 60 * 60 * 1000;
/// How long an unfinished claim blocks duplicates.
pub const CLAIM_TTL_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Outcome {
    Running,
    Done(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct IdemRecord {
    request: [u8; 32],
    outcome: Outcome,
    expires_at: Timestamp,
}

impl Record for IdemRecord {
    const TAG: &'static str = "idem";
}

/// What claiming an operation id found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    /// The id is new: do the work, then [`Stores::idem_finish`].
    Fresh,
    /// The request already finished with this response.
    Replay(Bytes),
    /// The same request is still running elsewhere.
    Running,
    /// The id was used for a different request.
    Mismatch,
}

impl Stores {
    /// Claim `operation` for `principal`'s request hashing to `request`.
    pub async fn idem_claim(
        &self,
        principal: &str,
        operation: &[u8],
        request: [u8; 32],
    ) -> Result<Claim> {
        let key = keys::idem(principal, operation);
        let now = Timestamp::now();
        let current = self.meta.get(&key).await?;
        if let Some(bytes) = &current {
            let record: IdemRecord = decode_record(bytes)?;
            let expired = record.expires_at <= now;
            if !expired {
                if record.request != request {
                    return Ok(Claim::Mismatch);
                }
                return Ok(match record.outcome {
                    Outcome::Done(response) => Claim::Replay(Bytes::from(response)),
                    Outcome::Running => Claim::Running,
                });
            }
        }
        let claim = IdemRecord {
            request,
            outcome: Outcome::Running,
            expires_at: Timestamp::from_millis(now.millis() + CLAIM_TTL_MS),
        };
        let tx = Transaction::new()
            .expect(key.clone(), current)
            .set(key, encode_record(&claim)?);
        match self.meta.transact(tx).await {
            Ok(_) => Ok(Claim::Fresh),
            // Someone claimed it between the read and the write.
            Err(Error::Conflict(_)) => Ok(Claim::Running),
            Err(e) => Err(e),
        }
    }

    /// The mutation recording `response` for a claimed operation, for the
    /// transaction doing the work so the two commit together.
    pub fn idem_done(
        &self,
        principal: &str,
        operation: &[u8],
        request: [u8; 32],
        response: &[u8],
    ) -> Result<Mutation> {
        let record = IdemRecord {
            request,
            outcome: Outcome::Done(response.to_vec()),
            expires_at: Timestamp::from_millis(Timestamp::now().millis() + IDEM_TTL_MS),
        };
        Ok(Mutation::Set {
            key: keys::idem(principal, operation),
            value: encode_record(&record)?,
        })
    }

    /// Record `response` for a claimed operation.
    pub async fn idem_finish(
        &self,
        principal: &str,
        operation: &[u8],
        request: [u8; 32],
        response: &[u8],
    ) -> Result<()> {
        let mut tx = Transaction::new();
        tx.mutations
            .push(self.idem_done(principal, operation, request, response)?);
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// Release a claim after the request failed.
    pub async fn idem_release(&self, principal: &str, operation: &[u8]) -> Result<()> {
        self.meta
            .transact(Transaction::new().delete(keys::idem(principal, operation)))
            .await?;
        Ok(())
    }

    /// Delete up to `limit` expired records; returns how many.
    pub async fn idem_sweep(&self, limit: usize) -> Result<usize> {
        let now = Timestamp::now();
        let rows = self.meta.scan(KeyRange::prefix(keys::idem_table())).await?;
        let mut tx = Transaction::new();
        let mut removed = 0;
        for (key, value) in rows {
            if removed == limit {
                break;
            }
            let record: IdemRecord = decode_record(&value)?;
            if record.expires_at <= now {
                tx = tx.expect(key.clone(), Some(value)).delete(key);
                removed += 1;
            }
        }
        if removed > 0 {
            self.meta.transact(tx).await?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn claims_replay_refuse_and_release() {
        let stores = Stores::memory();
        let (who, op, req) = ("pr_alice", b"op-1".as_slice(), [1u8; 32]);
        assert_eq!(stores.idem_claim(who, op, req).await.unwrap(), Claim::Fresh);
        assert_eq!(
            stores.idem_claim(who, op, req).await.unwrap(),
            Claim::Running
        );
        assert_eq!(
            stores.idem_claim(who, op, [2u8; 32]).await.unwrap(),
            Claim::Mismatch
        );
        stores.idem_finish(who, op, req, b"response").await.unwrap();
        assert_eq!(
            stores.idem_claim(who, op, req).await.unwrap(),
            Claim::Replay(Bytes::from_static(b"response"))
        );
        // Another principal's id space is separate.
        assert_eq!(
            stores.idem_claim("pr_bob", op, req).await.unwrap(),
            Claim::Fresh
        );
        // A released claim can be taken again.
        stores.idem_release("pr_bob", op).await.unwrap();
        assert_eq!(
            stores.idem_claim("pr_bob", op, req).await.unwrap(),
            Claim::Fresh
        );
        assert_eq!(stores.idem_sweep(100).await.unwrap(), 0);
    }
}
