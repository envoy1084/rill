//! Organizations: the tenants repositories belong to.
//!
//! An org's name is the namespace of every key of its repositories, so all
//! of an org's metadata is one key range. Names cannot change.

use bytes::Bytes;
use rill_types::{Principal, Record, RepoId, Timestamp};
use serde::{Deserialize, Serialize};

use super::codec::{decode_record, encode_record};
use super::stores::{RepoConfig, Stores};
use crate::error::{Error, Result};
use crate::keys;
use crate::meta::{KeyRange, Transaction};
use crate::seal::DEFAULT_NAMESPACE;

/// An organization record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgConfig {
    pub name: String,
    pub description: String,
    pub created_at: Timestamp,
}

impl Record for OrgConfig {
    const TAG: &'static str = "org";
}

/// The org every deployment has, used when a request names none.
pub const DEFAULT_ORG: &str = DEFAULT_NAMESPACE;

/// Org names are lowercase letters, digits and dashes, starting with a
/// letter or digit, at most 63 characters.
pub fn validate_org_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "org name {name:?}: use 1-63 lowercase letters, digits and dashes"
        )))
    }
}

/// Repository names are 1-128 printable characters without `/`.
pub fn validate_repo_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 128
        && !name.contains('/')
        && name.chars().all(|c| !c.is_control() && !c.is_whitespace());
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "repository name {name:?}: use 1-128 characters without '/' or spaces"
        )))
    }
}

impl Stores {
    /// Create an org. Fails with a conflict if it exists.
    pub async fn create_org(&self, name: &str, description: &str) -> Result<OrgConfig> {
        self.create_org_by(name, description, &Principal::service("rill"))
            .await
    }

    /// Create an org on `actor`'s authority.
    pub async fn create_org_by(
        &self,
        name: &str,
        description: &str,
        actor: &Principal,
    ) -> Result<OrgConfig> {
        validate_org_name(name)?;
        let org = OrgConfig {
            name: name.to_string(),
            description: description.to_string(),
            created_at: Timestamp::now(),
        };
        let key = keys::org(name);
        let mut tx = Transaction::new()
            .expect_absent(key.clone())
            .set(key, encode_record(&org)?);
        tx.mutations.extend(self.identity().log(
            rill_types::identity::IdentityChange::OrgCreated {
                org: name.to_string(),
            },
            actor,
        )?);
        self.meta.transact(tx).await.map_err(|e| match e {
            Error::Conflict(_) => Error::Conflict(format!("org {name:?} already exists")),
            other => other,
        })?;
        Ok(org)
    }

    /// Create the default org if it does not exist yet.
    pub async fn ensure_default_org(&self) -> Result<()> {
        match self.create_org(DEFAULT_ORG, "default organization").await {
            Ok(_) | Err(Error::Conflict(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn get_org(&self, name: &str) -> Result<Option<OrgConfig>> {
        match self.meta.get(&keys::org(name)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn list_orgs(&self) -> Result<Vec<OrgConfig>> {
        let rows = self.meta.scan(KeyRange::prefix(keys::org_list())).await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    /// The repository `id` in whichever org it belongs to.
    pub async fn find_repo(&self, id: &RepoId) -> Result<Option<RepoConfig>> {
        match self.meta.get(&keys::repo_org(id)).await? {
            Some(org) => {
                let org = std::str::from_utf8(&org)
                    .map_err(|_| Error::Codec("bad org name in repo index".into()))?;
                self.org(org).get_repo(id).await
            }
            None => Ok(None),
        }
    }

    /// The global index row of a new repository, written with it.
    pub(super) fn repo_org_row(&self, id: &RepoId) -> (Bytes, Bytes) {
        (
            keys::repo_org(id),
            Bytes::copy_from_slice(self.namespace().as_bytes()),
        )
    }
}
