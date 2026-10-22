//! Org secrets: values sealed under the org's data keys, written and
//! listed by administrators, opened only to inject into actions.

use bytes::Bytes;
use rill_store::repo::{decode_record, encode_record};
use rill_store::{KeyRange, Mutation, ObjectAt, Transaction};
use rill_types::{ObjectHash, ObjectKind, Principal, Record, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::remote::Scheduler;

/// Shortest secret value: shorter ones would mask ordinary output.
pub const MIN_SECRET_LEN: usize = 6;
/// Longest secret value.
pub const MAX_SECRET_LEN: usize = 64 * 1024;

/// Where a secret may be used.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretScope {
    /// Repositories whose plans may use it; empty is every repository.
    pub repos: Vec<String>,
    /// Only verification of trunk (after landing) gets it, never a
    /// change before it lands.
    pub trunk_only: bool,
}

/// A secret without its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretInfo {
    pub name: String,
    pub scope: SecretScope,
    pub updated_at: Timestamp,
    pub updated_by: Principal,
}

#[derive(Serialize, Deserialize)]
struct SecretRecord {
    name: String,
    sealed: Bytes,
    scope: SecretScope,
    updated_at: Timestamp,
    updated_by: Principal,
}

impl Record for SecretRecord {
    const TAG: &'static str = "exec-secret";
}

/// The namespace whose data keys seal `org`'s secrets.
fn namespace(org: &str) -> String {
    format!("{org}.secrets")
}

fn name_hash(name: &str) -> ObjectHash {
    ObjectHash::digest(ObjectKind::Derived, format!("secret:{name}").as_bytes())
}

/// Secret names are environment variable names.
pub fn validate_secret_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let valid = name.len() <= 128
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidAction(format!(
            "secret name {name:?} is not an environment variable name"
        )))
    }
}

impl Scheduler {
    /// Set `org`'s secret `name` to `value`, committing `log` (the audit
    /// record of the change) with it.
    pub async fn put_secret(
        &self,
        org: &str,
        name: &str,
        value: &[u8],
        scope: SecretScope,
        by: &Principal,
        log: Vec<Mutation>,
    ) -> Result<SecretInfo> {
        validate_secret_name(name)?;
        if !(MIN_SECRET_LEN..=MAX_SECRET_LEN).contains(&value.len()) {
            return Err(Error::InvalidAction(format!(
                "a secret value is {MIN_SECRET_LEN} to {MAX_SECRET_LEN} bytes"
            )));
        }
        let sealed = self.seal(&namespace(org), &name_hash(name), value).await?;
        let record = SecretRecord {
            name: name.to_string(),
            sealed: Bytes::from(sealed),
            scope,
            updated_at: Timestamp::now(),
            updated_by: by.clone(),
        };
        let info = SecretInfo {
            name: record.name.clone(),
            scope: record.scope.clone(),
            updated_at: record.updated_at,
            updated_by: record.updated_by.clone(),
        };
        let space = self.space(org);
        let mut tx = Transaction::new().set(
            space.records.keys().exec_secret(name),
            encode_record(&record)?,
        );
        tx.mutations.extend(log);
        space.records.meta().transact(tx).await?;
        Ok(info)
    }

    /// Remove `org`'s secret `name` with `log`; whether it existed.
    pub async fn delete_secret(&self, org: &str, name: &str, log: Vec<Mutation>) -> Result<bool> {
        let space = self.space(org);
        let key = space.records.keys().exec_secret(name);
        if space.records.meta().get(&key).await?.is_none() {
            return Ok(false);
        }
        let mut tx = Transaction::new().delete(key);
        tx.mutations.extend(log);
        space.records.meta().transact(tx).await?;
        Ok(true)
    }

    /// `org`'s secrets, by name, without values.
    pub async fn list_secrets(&self, org: &str) -> Result<Vec<SecretInfo>> {
        let space = self.space(org);
        let rows = space
            .records
            .meta()
            .scan(KeyRange::prefix(space.records.keys().exec_secrets()))
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for (_, value) in rows {
            let record: SecretRecord = decode_record(&value)?;
            out.push(SecretInfo {
                name: record.name,
                scope: record.scope,
                updated_at: record.updated_at,
                updated_by: record.updated_by,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// Why a plan of `repo` may not use `names`, if it may not: a secret
    /// that is not set, is limited to other repositories, or to trunk.
    pub async fn secret_refusal(
        &self,
        org: &str,
        names: &[String],
        repo: &str,
        trunk: bool,
    ) -> Result<Option<String>> {
        for name in names {
            let Some(record) = self.secret_record(org, name).await? else {
                return Ok(Some(format!("secret {name} is not set")));
            };
            if !record.scope.repos.is_empty() && !record.scope.repos.iter().any(|r| r == repo) {
                return Ok(Some(format!("secret {name} is not available to {repo}")));
            }
            if record.scope.trunk_only && !trunk {
                return Ok(Some(format!(
                    "secret {name} is only available after landing"
                )));
            }
        }
        Ok(None)
    }

    /// The values of `names` in `org`; a missing secret is an error.
    pub(crate) async fn open_secrets(
        &self,
        org: &str,
        names: &[String],
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            let record = self
                .secret_record(org, name)
                .await?
                .ok_or_else(|| Error::InvalidAction(format!("secret {name} is not set")))?;
            let value = self
                .open(&namespace(org), &name_hash(name), record.sealed.to_vec())
                .await?;
            out.push((name.clone(), value));
        }
        Ok(out)
    }

    async fn secret_record(&self, org: &str, name: &str) -> Result<Option<SecretRecord>> {
        let space = self.space(org);
        match space
            .records
            .meta()
            .get(&space.records.keys().exec_secret(name))
            .await?
        {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub(crate) async fn seal(
        &self,
        namespace: &str,
        hash: &ObjectHash,
        plain: &[u8],
    ) -> Result<Vec<u8>> {
        self.sealer.ready(namespace).await?;
        Ok(self.sealer.seal(at(namespace, hash), plain.to_vec())?)
    }

    pub(crate) async fn open(
        &self,
        namespace: &str,
        hash: &ObjectHash,
        sealed: Vec<u8>,
    ) -> Result<Vec<u8>> {
        self.sealer.ready(namespace).await?;
        Ok(self.sealer.open(at(namespace, hash), sealed)?)
    }
}

fn at<'a>(namespace: &'a str, hash: &'a ObjectHash) -> ObjectAt<'a> {
    ObjectAt {
        namespace,
        kind: ObjectKind::Derived,
        hash,
    }
}

/// The namespace that seals commands carrying `org`'s secrets.
pub(crate) fn command_namespace(org: &str) -> String {
    namespace(org)
}
