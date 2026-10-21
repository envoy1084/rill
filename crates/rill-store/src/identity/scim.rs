//! SCIM credentials.

use rill_types::identity::ScimCredential;
use rill_types::Timestamp;

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

impl IdentityStore {
    pub async fn create_scim_credential(&self, credential: &ScimCredential) -> Result<()> {
        let key = keys::identity_row(table::IDENT_SCIM_CREDENTIAL, credential.id.as_str());
        let tx = Transaction::new()
            .expect_absent(key.clone())
            .set(key, encode_record(credential)?);
        self.meta.transact(tx).await?;
        Ok(())
    }

    pub async fn scim_credential(&self, id: &str) -> Result<Option<ScimCredential>> {
        match self
            .meta
            .get(&keys::identity_row(table::IDENT_SCIM_CREDENTIAL, id))
            .await?
        {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn scim_credentials(&self, org: &str) -> Result<Vec<ScimCredential>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_table(
                table::IDENT_SCIM_CREDENTIAL,
            )))
            .await?;
        let all = rows
            .iter()
            .map(|(_, v)| decode_record::<ScimCredential>(v))
            .collect::<Result<Vec<_>>>()?;
        Ok(all.into_iter().filter(|c| c.org == org).collect())
    }

    /// Revoke a SCIM credential.
    pub async fn revoke_scim_credential(&self, id: &str) -> Result<()> {
        let key = keys::identity_row(table::IDENT_SCIM_CREDENTIAL, id);
        let current = self
            .meta
            .get(&key)
            .await?
            .ok_or_else(|| Error::NotFound(format!("SCIM credential {id}")))?;
        let mut credential: ScimCredential = decode_record(&current)?;
        credential.revoked_at.get_or_insert(Timestamp::now());
        let tx = Transaction::new()
            .expect(key.clone(), Some(current))
            .set(key, encode_record(&credential)?);
        self.meta.transact(tx).await?;
        Ok(())
    }
}
