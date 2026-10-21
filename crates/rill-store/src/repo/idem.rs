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
    /// Claimed and not finished. `effect` is what the request already
    /// committed (a version hash, say), written in the same transaction as
    /// that work, so a takeover after a crash does not redo it.
    Running {
        effect: Option<Vec<u8>>,
    },
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
    /// The id is new, or its claim expired: do the work, then
    /// [`Stores::idem_finish`]. `prior` is the effect an expired claim had
    /// already committed.
    Fresh { prior: Option<Bytes> },
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
        let mut prior = None;
        if let Some(bytes) = &current {
            let record: IdemRecord = decode_record(bytes)?;
            let expired = record.expires_at <= now;
            if !expired {
                if record.request != request {
                    return Ok(Claim::Mismatch);
                }
                return Ok(match record.outcome {
                    Outcome::Done(response) => Claim::Replay(Bytes::from(response)),
                    Outcome::Running { .. } => Claim::Running,
                });
            }
            if let (
                true,
                Outcome::Running {
                    effect: Some(effect),
                },
            ) = (record.request == request, record.outcome)
            {
                prior = Some(Bytes::from(effect));
            }
        }
        let claim = IdemRecord {
            request,
            outcome: Outcome::Running {
                effect: prior.as_ref().map(|b| b.to_vec()),
            },
            expires_at: Timestamp::from_millis(now.millis() + CLAIM_TTL_MS),
        };
        let tx = Transaction::new()
            .expect(key.clone(), current)
            .set(key, encode_record(&claim)?);
        match self.meta.transact(tx).await {
            Ok(_) => Ok(Claim::Fresh { prior }),
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

    /// The mutation noting that a claimed operation committed `effect`,
    /// for the transaction that commits it.
    pub fn idem_effect(
        &self,
        principal: &str,
        operation: &[u8],
        request: [u8; 32],
        effect: &[u8],
    ) -> Result<Mutation> {
        let record = IdemRecord {
            request,
            outcome: Outcome::Running {
                effect: Some(effect.to_vec()),
            },
            expires_at: Timestamp::from_millis(Timestamp::now().millis() + CLAIM_TTL_MS),
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

    /// Release a claim after the request failed. A claim whose request
    /// already committed its effect is not dropped: it expires at once, so
    /// a retry takes it over and learns the effect instead of redoing it.
    pub async fn idem_release(&self, principal: &str, operation: &[u8]) -> Result<()> {
        let key = keys::idem(principal, operation);
        let Some(bytes) = self.meta.get(&key).await? else {
            return Ok(());
        };
        let mut record: IdemRecord = decode_record(&bytes)?;
        let tx = match record.outcome {
            Outcome::Running { effect: Some(_) } => {
                record.expires_at = Timestamp::now();
                Transaction::new()
                    .expect(key.clone(), Some(bytes))
                    .set(key, encode_record(&record)?)
            }
            _ => Transaction::new()
                .expect(key.clone(), Some(bytes))
                .delete(key),
        };
        match self.meta.transact(tx).await {
            Ok(_) | Err(Error::Conflict(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Make a finished request look as if its server died after committing
    /// `effect` and before replying, for crash tests.
    #[cfg(feature = "testing")]
    pub async fn idem_rewind(
        &self,
        principal: &str,
        operation: &[u8],
        effect: &[u8],
    ) -> Result<()> {
        let key = keys::idem(principal, operation);
        let bytes = self
            .meta
            .get(&key)
            .await?
            .ok_or_else(|| Error::NotFound("no such operation".into()))?;
        let mut record: IdemRecord = decode_record(&bytes)?;
        record.outcome = Outcome::Running {
            effect: Some(effect.to_vec()),
        };
        record.expires_at = Timestamp::from_millis(0);
        self.meta.set(key, encode_record(&record)?).await
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
        assert_eq!(
            stores.idem_claim(who, op, req).await.unwrap(),
            Claim::Fresh { prior: None }
        );
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
            Claim::Fresh { prior: None }
        );
        // A released claim can be taken again.
        stores.idem_release("pr_bob", op).await.unwrap();
        assert_eq!(
            stores.idem_claim("pr_bob", op, req).await.unwrap(),
            Claim::Fresh { prior: None }
        );
        assert_eq!(stores.idem_sweep(100).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn a_takeover_after_a_crash_learns_what_was_committed() {
        let stores = Stores::memory();
        let (who, op, req) = ("pr_alice", b"op-2".as_slice(), [3u8; 32]);
        stores.idem_claim(who, op, req).await.unwrap();
        let mut tx = Transaction::new();
        tx.mutations
            .push(stores.idem_effect(who, op, req, b"version-hash").unwrap());
        stores.meta.transact(tx).await.unwrap();
        // The server dies; the claim expires.
        let expired = IdemRecord {
            request: req,
            outcome: Outcome::Running {
                effect: Some(b"version-hash".to_vec()),
            },
            expires_at: Timestamp::from_millis(0),
        };
        stores
            .meta
            .set(keys::idem(who, op), encode_record(&expired).unwrap())
            .await
            .unwrap();
        assert_eq!(
            stores.idem_claim(who, op, req).await.unwrap(),
            Claim::Fresh {
                prior: Some(Bytes::from_static(b"version-hash"))
            }
        );
        assert_eq!(
            stores.idem_sweep(100).await.unwrap(),
            0,
            "the new claim is live"
        );
    }
}
