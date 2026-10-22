//! Teams (SCIM groups) of an organization.

use bytes::Bytes;
use rill_types::identity::{IdentityChange, TeamRecord};
use rill_types::Principal;

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

fn team_key(org: &str, name: &str) -> Bytes {
    keys::identity_pair(table::IDENT_TEAM, org, name)
}

fn scim_key(org: &str, scim_id: &str) -> Bytes {
    keys::identity_pair(table::IDENT_SCIM_GROUP, org, scim_id)
}

fn changed(team: &TeamRecord) -> IdentityChange {
    IdentityChange::TeamChanged {
        org: team.org.clone(),
        name: team.name.clone(),
    }
}

impl IdentityStore {
    pub async fn team(&self, org: &str, name: &str) -> Result<Option<TeamRecord>> {
        match self.meta.get(&team_key(org, name)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn team_by_scim_id(&self, org: &str, scim_id: &str) -> Result<Option<TeamRecord>> {
        match self.meta.get(&scim_key(org, scim_id)).await? {
            Some(name) => {
                let name = String::from_utf8(name.to_vec())
                    .map_err(|_| Error::Codec("bad team name in SCIM index".into()))?;
                self.team(org, &name).await
            }
            None => Ok(None),
        }
    }

    pub async fn teams(&self, org: &str) -> Result<Vec<TeamRecord>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_row(table::IDENT_TEAM, org)))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    /// Create a team; a conflict when the name or SCIM id is taken.
    pub async fn create_team(&self, team: &TeamRecord, actor: &Principal) -> Result<()> {
        let key = team_key(&team.org, &team.name);
        let tx = Transaction::new()
            .expect_absent(key.clone())
            .expect_absent(scim_key(&team.org, &team.scim_id))
            .set(key, encode_record(team)?)
            .set(
                scim_key(&team.org, &team.scim_id),
                Bytes::from(team.name.clone()),
            );
        let tx = self.logged(tx, changed(team), actor)?;
        self.meta.transact(tx).await.map_err(|e| match e {
            Error::Conflict(_) => Error::Conflict(format!("a team named {:?} exists", team.name)),
            other => other,
        })?;
        Ok(())
    }

    /// Replace `before` with `after` (the same SCIM id; it may be renamed).
    pub async fn update_team(
        &self,
        before: &TeamRecord,
        after: &TeamRecord,
        actor: &Principal,
    ) -> Result<()> {
        if before.org != after.org || before.scim_id != after.scim_id {
            return Err(Error::Invalid(
                "a team's org and SCIM id cannot change".into(),
            ));
        }
        let old_key = team_key(&before.org, &before.name);
        let mut tx = Transaction::new().expect(old_key.clone(), Some(encode_record(before)?));
        if before.name != after.name {
            let new_key = team_key(&after.org, &after.name);
            tx = tx
                .delete(old_key)
                .expect_absent(new_key.clone())
                .set(new_key, encode_record(after)?)
                .set(
                    scim_key(&after.org, &after.scim_id),
                    Bytes::from(after.name.clone()),
                );
        } else {
            tx = tx.set(old_key, encode_record(after)?);
        }
        let tx = self.logged(tx, changed(after), actor)?;
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// Delete a team; returns whether it existed.
    pub async fn delete_team(&self, team: &TeamRecord, actor: &Principal) -> Result<bool> {
        let key = team_key(&team.org, &team.name);
        if self.meta.get(&key).await?.is_none() {
            return Ok(false);
        }
        let tx = Transaction::new()
            .delete(key)
            .delete(scim_key(&team.org, &team.scim_id));
        let tx = self.logged(tx, changed(team), actor)?;
        self.meta.transact(tx).await?;
        Ok(true)
    }
}
