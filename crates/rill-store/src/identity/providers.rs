//! An organization's identity providers, indexed by issuer.

use bytes::Bytes;
use rill_types::identity::{IdentityChange, IdentityProvider};
use rill_types::Principal;

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

fn provider_key(org: &str, name: &str) -> Bytes {
    keys::identity_pair(table::IDENT_PROVIDER, org, name)
}

fn issuer_key(issuer: &str) -> Bytes {
    keys::identity_row(table::IDENT_PROVIDER_ISSUER, issuer)
}

impl IdentityStore {
    pub async fn provider(&self, org: &str, name: &str) -> Result<Option<IdentityProvider>> {
        match self.meta.get(&provider_key(org, name)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    /// The provider whose issuer is `issuer`; an issuer belongs to one
    /// provider in the instance.
    pub async fn provider_by_issuer(&self, issuer: &str) -> Result<Option<IdentityProvider>> {
        match self.meta.get(&issuer_key(issuer)).await? {
            Some(pair) => {
                let (org, name): (String, String) = foundationdb_tuple::unpack(&pair)
                    .map_err(|e| Error::Codec(format!("provider index: {e}")))?;
                self.provider(&org, &name).await
            }
            None => Ok(None),
        }
    }

    pub async fn providers(&self, org: &str) -> Result<Vec<IdentityProvider>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_row(
                table::IDENT_PROVIDER,
                org,
            )))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    /// Create or replace a provider. Fails with a conflict when another
    /// provider has the same issuer.
    pub async fn put_provider(&self, provider: &IdentityProvider, actor: &Principal) -> Result<()> {
        let key = provider_key(&provider.org, &provider.name);
        let pair = Bytes::from(foundationdb_tuple::pack(&(
            provider.org.as_str(),
            provider.name.as_str(),
        )));
        let mut tx = Transaction::new().set(key.clone(), encode_record(provider)?);
        match self.provider(&provider.org, &provider.name).await? {
            Some(old) if old.issuer == provider.issuer => {
                tx = tx.expect(issuer_key(&provider.issuer), Some(pair.clone()));
            }
            Some(old) => {
                tx = tx
                    .delete(issuer_key(&old.issuer))
                    .expect_absent(issuer_key(&provider.issuer));
            }
            None => tx = tx.expect_absent(issuer_key(&provider.issuer)),
        }
        let tx = tx.set(issuer_key(&provider.issuer), pair);
        let tx = self.logged(
            tx,
            IdentityChange::ProviderChanged {
                org: provider.org.clone(),
                name: provider.name.clone(),
            },
            actor,
        )?;
        self.meta.transact(tx).await.map_err(|e| match e {
            Error::Conflict(_) => Error::Conflict(format!(
                "another provider already uses issuer {:?}",
                provider.issuer
            )),
            other => other,
        })?;
        Ok(())
    }

    /// Remove a provider; returns whether it existed.
    pub async fn delete_provider(&self, org: &str, name: &str, actor: &Principal) -> Result<bool> {
        let Some(old) = self.provider(org, name).await? else {
            return Ok(false);
        };
        let tx = Transaction::new()
            .delete(provider_key(org, name))
            .delete(issuer_key(&old.issuer));
        let tx = self.logged(
            tx,
            IdentityChange::ProviderChanged {
                org: org.to_string(),
                name: name.to_string(),
            },
            actor,
        )?;
        self.meta.transact(tx).await?;
        Ok(true)
    }
}
