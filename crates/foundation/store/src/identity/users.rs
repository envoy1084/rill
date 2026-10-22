//! Users of an organization, with the indexes that keep principal names
//! unique across the instance and map external identities to users.

use bytes::Bytes;
use rill_types::identity::{ExternalIdentity, IdentityChange, UserRecord};
use rill_types::Principal;

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

fn user_key(org: &str, name: &str) -> Bytes {
    keys::identity_pair(table::IDENT_USER, org, name)
}

fn principal_key(principal: &Principal) -> Bytes {
    keys::identity_pair(
        table::IDENT_PRINCIPAL,
        principal.kind.as_str(),
        principal.name.as_str(),
    )
}

fn external_key(id: &ExternalIdentity) -> Bytes {
    keys::identity_pair(
        table::IDENT_EXTERNAL,
        id.issuer.as_str(),
        id.subject.as_str(),
    )
}

fn scim_key(org: &str, scim_id: &str) -> Bytes {
    keys::identity_pair(table::IDENT_SCIM_USER, org, scim_id)
}

fn encode_pair(org: &str, name: &str) -> Bytes {
    Bytes::from(foundationdb_tuple::pack(&(org, name)))
}

fn decode_pair(bytes: &[u8]) -> Result<(String, String)> {
    foundationdb_tuple::unpack(bytes).map_err(|e| Error::Codec(format!("identity index: {e}")))
}

impl IdentityStore {
    pub async fn user(&self, org: &str, name: &str) -> Result<Option<UserRecord>> {
        match self.meta.get(&user_key(org, name)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    /// The user an external identity signs in as.
    pub async fn user_by_external(&self, id: &ExternalIdentity) -> Result<Option<UserRecord>> {
        match self.meta.get(&external_key(id)).await? {
            Some(pair) => {
                let (org, name) = decode_pair(&pair)?;
                self.user(&org, &name).await
            }
            None => Ok(None),
        }
    }

    /// The org a principal name belongs to, if any.
    pub async fn principal_org(&self, principal: &Principal) -> Result<Option<String>> {
        match self.meta.get(&principal_key(principal)).await? {
            Some(org) => {
                Ok(Some(String::from_utf8(org.to_vec()).map_err(|_| {
                    Error::Codec("bad org in principal index".into())
                })?))
            }
            None => Ok(None),
        }
    }

    /// The user a provisioning system knows as `scim_id`.
    pub async fn user_by_scim_id(&self, org: &str, scim_id: &str) -> Result<Option<UserRecord>> {
        match self.meta.get(&scim_key(org, scim_id)).await? {
            Some(name) => {
                let name = String::from_utf8(name.to_vec())
                    .map_err(|_| Error::Codec("bad user name in SCIM index".into()))?;
                self.user(org, &name).await
            }
            None => Ok(None),
        }
    }

    pub async fn users(&self, org: &str) -> Result<Vec<UserRecord>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_row(table::IDENT_USER, org)))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    /// Store a new user. Fails with a conflict when the principal name is
    /// taken in any org, or an external identity already signs in as
    /// someone.
    pub async fn create_user(&self, user: &UserRecord, actor: &Principal) -> Result<()> {
        let key = user_key(&user.org, &user.principal.name);
        let mut tx = Transaction::new()
            .expect_absent(key.clone())
            .expect_absent(principal_key(&user.principal))
            .set(key, encode_record(user)?)
            .set(
                principal_key(&user.principal),
                Bytes::from(user.org.clone()),
            );
        for id in &user.external {
            tx = tx.expect_absent(external_key(id)).set(
                external_key(id),
                encode_pair(&user.org, &user.principal.name),
            );
        }
        if let Some(scim) = &user.scim_id {
            tx = tx.expect_absent(scim_key(&user.org, scim)).set(
                scim_key(&user.org, scim),
                Bytes::from(user.principal.name.clone()),
            );
        }
        let tx = self.logged(tx, user_changed(user), actor)?;
        self.meta.transact(tx).await.map_err(|e| match e {
            Error::Conflict(_) => Error::Conflict(format!(
                "a principal named {:?} or one of its identities exists",
                user.principal.name
            )),
            other => other,
        })?;
        Ok(())
    }

    /// Replace `before` with `after` (same org and name), moving the
    /// external identity index with it. Fails with a conflict when the
    /// user changed since `before` was read.
    pub async fn update_user(
        &self,
        before: &UserRecord,
        after: &UserRecord,
        actor: &Principal,
    ) -> Result<()> {
        if before.org != after.org || before.principal != after.principal {
            return Err(Error::Invalid("a user's org and name cannot change".into()));
        }
        let key = user_key(&after.org, &after.principal.name);
        let mut tx = Transaction::new()
            .expect(key.clone(), Some(encode_record(before)?))
            .set(key, encode_record(after)?);
        for id in before
            .external
            .iter()
            .filter(|id| !after.external.contains(id))
        {
            tx = tx.delete(external_key(id));
        }
        for id in after
            .external
            .iter()
            .filter(|id| !before.external.contains(id))
        {
            tx = tx.expect_absent(external_key(id)).set(
                external_key(id),
                encode_pair(&after.org, &after.principal.name),
            );
        }
        if before.scim_id != after.scim_id {
            if let Some(old) = &before.scim_id {
                tx = tx.delete(scim_key(&before.org, old));
            }
            if let Some(new) = &after.scim_id {
                tx = tx.expect_absent(scim_key(&after.org, new)).set(
                    scim_key(&after.org, new),
                    Bytes::from(after.principal.name.clone()),
                );
            }
        }
        let tx = self.logged(tx, user_changed(after), actor)?;
        self.meta.transact(tx).await?;
        Ok(())
    }
}

fn user_changed(user: &UserRecord) -> IdentityChange {
    IdentityChange::UserChanged {
        org: user.org.clone(),
        name: user.principal.name.clone(),
    }
}
