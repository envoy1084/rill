//! The pair of stores a deployment runs on, and the repositories in it.

use std::sync::Arc;

use bytes::Bytes;
use rill_types::{Record, RefName, RepoId, Timestamp};
use serde::{Deserialize, Serialize};

use super::codec::{decode_record, encode_record};
use super::store::RepoStore;
use crate::error::{Error, Result};
use crate::keys;
use crate::meta::{KeyRange, MetaStore, Transaction};
use crate::object::ObjectStore;
use crate::seal::DEFAULT_NAMESPACE;

/// Per-repository configuration record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoConfig {
    pub id: RepoId,
    pub name: String,
    pub description: String,
    pub created_at: Timestamp,
    /// Trunk ref name; always `trunk` today but stored so it can change.
    pub trunk: RefName,
    pub settings: RepoSettings,
}

/// Per-repository rules chosen at creation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSettings {
    /// Reject paths Windows cannot create.
    pub windows_safe: bool,
}

impl Record for RepoConfig {
    const TAG: &'static str = "repo";
}

/// The pair of backends a deployment is built on, and the namespace its
/// repositories live in.
#[derive(Clone)]
pub struct Stores {
    pub objects: Arc<dyn ObjectStore>,
    pub meta: Arc<dyn MetaStore>,
    namespace: Arc<str>,
}

/// Format of everything a store holds. Changing any encoding bumps it, and
/// a build opens only stores of its own format.
pub const STORE_FORMAT: u32 = 1;

impl Stores {
    /// Stores over `objects` (wrapped so every read is hash-checked) and
    /// `meta`, in the default namespace.
    pub fn new(objects: Arc<dyn ObjectStore>, meta: Arc<dyn MetaStore>) -> Self {
        Stores {
            objects: Arc::new(crate::verified::VerifiedObjectStore::new(objects)),
            meta,
            namespace: Arc::from(DEFAULT_NAMESPACE),
        }
    }

    pub fn memory() -> Self {
        Stores::new(
            Arc::new(crate::memory::MemoryObjectStore::new()),
            Arc::new(crate::memory::MemoryMetaStore::new()),
        )
    }

    /// The namespace every key of these stores is under.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Stamp an empty store with [`STORE_FORMAT`], or refuse a store
    /// written in another format.
    pub async fn check_format(&self) -> Result<()> {
        let key = keys::store_format();
        match self.meta.get(&key).await? {
            Some(bytes) => {
                let found = <[u8; 4]>::try_from(bytes.as_ref())
                    .map(u32::from_be_bytes)
                    .map_err(|_| Error::Codec("malformed store format marker".into()))?;
                if found != STORE_FORMAT {
                    return Err(Error::IncompatibleFormat {
                        found,
                        expected: STORE_FORMAT,
                    });
                }
                Ok(())
            }
            None => {
                let tx = Transaction::new()
                    .expect_absent(key.clone())
                    .set(key, Bytes::copy_from_slice(&STORE_FORMAT.to_be_bytes()));
                match self.meta.transact(tx).await {
                    Ok(_) | Err(Error::Conflict(_)) => Ok(()),
                    Err(e) => Err(e),
                }
            }
        }
    }

    pub fn repo(&self, id: RepoId) -> RepoStore {
        RepoStore::new(self.clone(), id)
    }

    pub async fn create_repo(&self, name: &str, description: &str) -> Result<RepoConfig> {
        self.create_repo_with(name, description, RepoSettings::default())
            .await
    }

    /// [`Stores::create_repo`] with explicit settings.
    pub async fn create_repo_with(
        &self,
        name: &str,
        description: &str,
        settings: RepoSettings,
    ) -> Result<RepoConfig> {
        let config = RepoConfig {
            id: RepoId::new(),
            name: name.to_string(),
            description: description.to_string(),
            created_at: Timestamp::now(),
            trunk: RefName::trunk(),
            settings,
        };
        let ns = self.namespace();
        let tx = Transaction::new()
            .expect_absent(keys::repo_by_name(ns, name))
            .set(keys::repo(ns, &config.id), encode_record(&config)?)
            .set(
                keys::repo_by_name(ns, name),
                Bytes::copy_from_slice(config.id.as_bytes()),
            );
        self.meta.transact(tx).await.map_err(|e| match e {
            Error::Conflict(_) => Error::Conflict(format!("repo {name:?} already exists")),
            other => other,
        })?;
        Ok(config)
    }

    pub async fn get_repo(&self, id: &RepoId) -> Result<Option<RepoConfig>> {
        match self.meta.get(&keys::repo(self.namespace(), id)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn get_repo_by_name(&self, name: &str) -> Result<Option<RepoConfig>> {
        match self
            .meta
            .get(&keys::repo_by_name(self.namespace(), name))
            .await?
        {
            Some(bytes) => {
                let arr: [u8; 16] = bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| Error::Codec("bad repo id".into()))?;
                self.get_repo(&RepoId::from_bytes(arr)).await
            }
            None => Ok(None),
        }
    }

    pub async fn list_repos(&self) -> Result<Vec<RepoConfig>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::repo_list(self.namespace())))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn repo_names_are_unique() {
        let stores = Stores::memory();
        stores.create_repo("r", "").await.unwrap();
        assert!(stores.create_repo("r", "").await.unwrap_err().is_conflict());
        assert!(stores.get_repo_by_name("r").await.unwrap().is_some());
        assert_eq!(stores.list_repos().await.unwrap().len(), 1);
    }
}
