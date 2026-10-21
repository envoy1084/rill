//! Token signing keys.

use rill_types::identity::{IdentityChange, TokenKeyRecord};
use rill_types::{Principal, Timestamp};

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

impl IdentityStore {
    /// Every stored signing key, oldest id first.
    pub async fn token_keys(&self) -> Result<Vec<TokenKeyRecord>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_table(table::IDENT_KEY)))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    /// Store a new key. A conflict means another server added key
    /// `key.id` first.
    pub async fn add_token_key(&self, key: &TokenKeyRecord, actor: &Principal) -> Result<()> {
        let row = keys::identity_row(table::IDENT_KEY, key.id as u64);
        let tx = Transaction::new()
            .expect_absent(row.clone())
            .set(row, encode_record(key)?);
        let tx = self.logged(tx, IdentityChange::KeysChanged, actor)?;
        self.meta.transact(tx).await.map_err(|e| match e {
            Error::Conflict(_) => Error::Conflict(format!("token key {} already exists", key.id)),
            other => other,
        })?;
        Ok(())
    }

    /// Retire key `id` at `not_after`, unless it already retires sooner.
    pub async fn retire_token_key(
        &self,
        id: u32,
        not_after: Timestamp,
        actor: &Principal,
    ) -> Result<()> {
        let row = keys::identity_row(table::IDENT_KEY, id as u64);
        let current = self
            .meta
            .get(&row)
            .await?
            .ok_or_else(|| Error::NotFound(format!("token key {id}")))?;
        let mut key: TokenKeyRecord = decode_record(&current)?;
        if key.not_after.is_some_and(|t| t <= not_after) {
            return Ok(());
        }
        key.not_after = Some(not_after);
        let tx = Transaction::new()
            .expect(row.clone(), Some(current))
            .set(row, encode_record(&key)?);
        let tx = self.logged(tx, IdentityChange::KeysChanged, actor)?;
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// Delete keys retired before `before`; returns how many.
    pub async fn prune_token_keys(&self, before: Timestamp, actor: &Principal) -> Result<usize> {
        let mut tx = Transaction::new();
        let mut removed = 0;
        for key in self.token_keys().await? {
            if key.not_after.is_some_and(|t| t < before) {
                tx = tx.delete(keys::identity_row(table::IDENT_KEY, key.id as u64));
                removed += 1;
            }
        }
        if removed > 0 {
            let tx = self.logged(tx, IdentityChange::KeysChanged, actor)?;
            self.meta.transact(tx).await?;
        }
        Ok(removed)
    }
}
