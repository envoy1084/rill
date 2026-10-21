//! Sessions.

use rill_types::identity::{IdentityChange, SessionRecord};
use rill_types::{Principal, SessionId, Timestamp};

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

fn row(id: &SessionId) -> bytes::Bytes {
    keys::identity_row(table::IDENT_SESSION, id.as_bytes().as_slice())
}

impl IdentityStore {
    /// Store a new session.
    pub async fn create_session(&self, session: &SessionRecord) -> Result<()> {
        let key = row(&session.id);
        let tx = Transaction::new()
            .expect_absent(key.clone())
            .set(key, encode_record(session)?);
        self.meta.transact(tx).await?;
        Ok(())
    }

    pub async fn session(&self, id: &SessionId) -> Result<Option<SessionRecord>> {
        match self.meta.get(&row(id)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Revoke session `id`; every token issued under it is refused from
    /// then on. Revoking a revoked session is a no-op.
    pub async fn revoke_session(&self, id: &SessionId, actor: &Principal) -> Result<()> {
        let key = row(id);
        let current = self
            .meta
            .get(&key)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {id}")))?;
        let mut session: SessionRecord = decode_record(&current)?;
        if session.revoked_at.is_some() {
            return Ok(());
        }
        session.revoked_at = Some(Timestamp::now());
        let tx = Transaction::new()
            .expect(key.clone(), Some(current))
            .set(key, encode_record(&session)?);
        let tx = self.logged(tx, IdentityChange::SessionRevoked { session: *id }, actor)?;
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// Sessions still live at `now`, up to `limit`.
    pub async fn live_sessions(&self, now: Timestamp, limit: usize) -> Result<Vec<SessionRecord>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_table(table::IDENT_SESSION)))
            .await?;
        let mut out = Vec::new();
        for (_, v) in rows {
            let session: SessionRecord = decode_record(&v)?;
            if session.is_live_at(now) {
                out.push(session);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Revoked sessions whose tokens could still be unexpired at `now`:
    /// what a server's view must refuse.
    pub async fn revoked_sessions(&self, now: Timestamp) -> Result<Vec<SessionRecord>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_table(table::IDENT_SESSION)))
            .await?;
        let mut out = Vec::new();
        for (_, v) in rows {
            let session: SessionRecord = decode_record(&v)?;
            if session.revoked_at.is_some() && now < session.expires_at {
                out.push(session);
            }
        }
        Ok(out)
    }

    /// Delete sessions that ended before `before`; returns how many.
    pub async fn sweep_sessions(&self, before: Timestamp, limit: usize) -> Result<usize> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_table(table::IDENT_SESSION)))
            .await?;
        let mut tx = Transaction::new();
        let mut removed = 0;
        for (key, v) in rows {
            let session: SessionRecord = decode_record(&v)?;
            if session.expires_at < before {
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
