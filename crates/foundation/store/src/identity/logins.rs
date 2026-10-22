//! Browser sign-ins in flight.

use rill_types::identity::PendingLogin;
use rill_types::Timestamp;

use super::store::IdentityStore;
use crate::error::Result;
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

impl IdentityStore {
    pub async fn put_pending_login(&self, login: &PendingLogin) -> Result<()> {
        let key = keys::identity_row(table::IDENT_LOGIN, login.state.as_str());
        let tx = Transaction::new()
            .expect_absent(key.clone())
            .set(key, encode_record(login)?);
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// Read and delete the sign-in `state` names, so a callback can be
    /// used once. `None` when unknown, used or expired at `now`.
    pub async fn take_pending_login(
        &self,
        state: &str,
        now: Timestamp,
    ) -> Result<Option<PendingLogin>> {
        let key = keys::identity_row(table::IDENT_LOGIN, state);
        let Some(bytes) = self.meta.get(&key).await? else {
            return Ok(None);
        };
        let tx = Transaction::new()
            .expect(key.clone(), Some(bytes.clone()))
            .delete(key);
        match self.meta.transact(tx).await {
            Ok(_) => {}
            Err(crate::Error::Conflict(_)) => return Ok(None),
            Err(e) => return Err(e),
        }
        let login: PendingLogin = decode_record(&bytes)?;
        Ok((now < login.expires_at).then_some(login))
    }

    /// Delete sign-ins that expired before `before`.
    pub async fn sweep_pending_logins(&self, before: Timestamp, limit: usize) -> Result<usize> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_table(table::IDENT_LOGIN)).limit(limit))
            .await?;
        let mut tx = Transaction::new();
        let mut removed = 0;
        for (key, v) in rows {
            let login: PendingLogin = decode_record(&v)?;
            if login.expires_at < before {
                tx = tx.delete(key);
                removed += 1;
            }
        }
        if removed > 0 {
            self.meta.transact(tx).await?;
        }
        Ok(removed)
    }
}
