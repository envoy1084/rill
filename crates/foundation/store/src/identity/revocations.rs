//! Revoked tokens.

use rill_types::identity::{IdentityChange, Revocation};
use rill_types::{Principal, Timestamp};

use super::store::IdentityStore;
use crate::error::Result;
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

impl IdentityStore {
    /// Revoke a token and every token attenuated from it.
    pub async fn revoke_token(&self, revocation: &Revocation, actor: &Principal) -> Result<()> {
        let key = keys::identity_row(
            table::IDENT_REVOKED,
            revocation.token_id.as_bytes().as_slice(),
        );
        let tx = Transaction::new().set(key, encode_record(revocation)?);
        let tx = self.logged(
            tx,
            IdentityChange::TokenRevoked {
                token_id: revocation.token_id,
                expires_at: revocation.expires_at,
            },
            actor,
        )?;
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// Revocations of tokens that have not expired at `now`.
    pub async fn revocations(&self, now: Timestamp) -> Result<Vec<Revocation>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_table(table::IDENT_REVOKED)))
            .await?;
        let mut out = Vec::new();
        for (_, v) in rows {
            let revocation: Revocation = decode_record(&v)?;
            if now < revocation.expires_at {
                out.push(revocation);
            }
        }
        Ok(out)
    }

    /// Delete revocations of tokens that expired before `before`.
    pub async fn sweep_revocations(&self, before: Timestamp, limit: usize) -> Result<usize> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_table(table::IDENT_REVOKED)))
            .await?;
        let mut tx = Transaction::new();
        let mut removed = 0;
        for (key, v) in rows {
            let revocation: Revocation = decode_record(&v)?;
            if revocation.expires_at < before {
                tx = tx.delete(key);
                removed += 1;
                if removed >= limit {
                    break;
                }
            }
        }
        if removed > 0 {
            self.meta.transact(tx).await?;
        }
        Ok(removed)
    }
}
