//! Workload issuers an organization trusts, and its identity settings.

use bytes::Bytes;
use rill_types::identity::{IdentityChange, OrgIdentitySettings, WorkloadIssuer};
use rill_types::Principal;

use super::store::IdentityStore;
use crate::error::Result;
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

fn issuer_key(org: &str, name: &str) -> Bytes {
    keys::identity_pair(table::IDENT_WORKLOAD, org, name)
}

impl IdentityStore {
    pub async fn workload_issuers(&self, org: &str) -> Result<Vec<WorkloadIssuer>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_row(
                table::IDENT_WORKLOAD,
                org,
            )))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    /// Create or replace a workload issuer.
    pub async fn put_workload_issuer(
        &self,
        issuer: &WorkloadIssuer,
        actor: &Principal,
    ) -> Result<()> {
        let tx = Transaction::new().set(
            issuer_key(&issuer.org, &issuer.name),
            encode_record(issuer)?,
        );
        let tx = self.logged(
            tx,
            IdentityChange::WorkloadIssuerChanged {
                org: issuer.org.clone(),
                name: issuer.name.clone(),
            },
            actor,
        )?;
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// Remove a workload issuer; returns whether it existed.
    pub async fn delete_workload_issuer(
        &self,
        org: &str,
        name: &str,
        actor: &Principal,
    ) -> Result<bool> {
        let key = issuer_key(org, name);
        if self.meta.get(&key).await?.is_none() {
            return Ok(false);
        }
        let tx = self.logged(
            Transaction::new().delete(key),
            IdentityChange::WorkloadIssuerChanged {
                org: org.to_string(),
                name: name.to_string(),
            },
            actor,
        )?;
        self.meta.transact(tx).await?;
        Ok(true)
    }

    /// `org`'s identity settings; the defaults when none were stored.
    pub async fn org_settings(&self, org: &str) -> Result<OrgIdentitySettings> {
        match self
            .meta
            .get(&keys::identity_row(table::IDENT_ORG_SETTINGS, org))
            .await?
        {
            Some(bytes) => decode_record(&bytes),
            None => Ok(OrgIdentitySettings {
                org: org.to_string(),
                ..OrgIdentitySettings::default()
            }),
        }
    }

    pub async fn put_org_settings(
        &self,
        settings: &OrgIdentitySettings,
        actor: &Principal,
    ) -> Result<()> {
        let tx = Transaction::new().set(
            keys::identity_row(table::IDENT_ORG_SETTINGS, settings.org.as_str()),
            encode_record(settings)?,
        );
        let tx = self.logged(
            tx,
            IdentityChange::OrgSettingsChanged {
                org: settings.org.clone(),
            },
            actor,
        )?;
        self.meta.transact(tx).await?;
        Ok(())
    }
}
