//! Each org's data keys, wrapped.

use rill_types::identity::{DataKeyRecord, IdentityChange};
use rill_types::Principal;

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

impl IdentityStore {
    /// `org`'s data keys, oldest first.
    pub async fn data_keys(&self, org: &str) -> Result<Vec<DataKeyRecord>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_row(table::DATA_KEY, org)))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    /// Every org's data keys.
    pub async fn all_data_keys(&self) -> Result<Vec<DataKeyRecord>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_table(table::DATA_KEY)))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    /// Store a new key. A conflict means another server added key
    /// `key.id` of the org first.
    pub async fn add_data_key(&self, key: &DataKeyRecord, actor: &Principal) -> Result<()> {
        let row = keys::identity_pair(table::DATA_KEY, key.org.as_str(), key.id);
        let tx = Transaction::new()
            .expect_absent(row.clone())
            .set(row, encode_record(key)?);
        let tx = self.logged(
            tx,
            IdentityChange::DataKeysChanged {
                org: key.org.clone(),
            },
            actor,
        )?;
        self.meta.transact(tx).await.map_err(|e| match e {
            Error::Conflict(_) => {
                Error::Conflict(format!("data key {} of {} already exists", key.id, key.org))
            }
            other => other,
        })?;
        Ok(())
    }

    /// Replace `old` with `new` (the same key, wrapped again), unless it
    /// changed since it was read.
    pub async fn rewrap_data_key(
        &self,
        old: &DataKeyRecord,
        new: &DataKeyRecord,
        actor: &Principal,
    ) -> Result<()> {
        if (old.org.as_str(), old.id) != (new.org.as_str(), new.id) {
            return Err(Error::Invalid("rewrapping changes only the wrap".into()));
        }
        let row = keys::identity_pair(table::DATA_KEY, old.org.as_str(), old.id);
        let tx = Transaction::new()
            .expect(row.clone(), Some(encode_record(old)?))
            .set(row, encode_record(new)?);
        let tx = self.logged(
            tx,
            IdentityChange::DataKeysChanged {
                org: old.org.clone(),
            },
            actor,
        )?;
        self.meta.transact(tx).await?;
        Ok(())
    }
}
