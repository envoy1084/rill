//! The persisted git-sha → change-version mapping.

use std::collections::BTreeMap;

use rill_store::repo::{decode_tagged, encode_tagged};
use rill_store::RepoStore;
use rill_types::ChangeVersionHash;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::reader::GitSha;

/// Name of the misc record holding the map.
pub const IMPORT_MAP_KEY: &str = "git_import_map";

/// Which git commits have been imported and as what.
///
/// Stored as one postcard record. Rewriting it is O(imported commits), so
/// the importer saves it in batches rather than per commit; an interrupted
/// import redoes at most one batch, and re-importing a commit that already
/// exists is idempotent anyway because change versions are content
/// addressed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportMap {
    entries: BTreeMap<GitSha, ChangeVersionHash>,
}

impl ImportMap {
    /// An empty map.
    pub fn new() -> Self {
        ImportMap::default()
    }

    /// Load the map stored in `repo`, or an empty one.
    pub async fn load(repo: &RepoStore) -> Result<Self> {
        match repo.get_misc(IMPORT_MAP_KEY).await? {
            Some(bytes) => {
                decode_tagged(IMPORT_MAP_KEY, &bytes).map_err(|e| Error::Codec(e.to_string()))
            }
            None => Ok(ImportMap::new()),
        }
    }

    /// Store the map in `repo`.
    pub async fn save(&self, repo: &RepoStore) -> Result<()> {
        let bytes = encode_tagged(IMPORT_MAP_KEY, self).map_err(|e| Error::Codec(e.to_string()))?;
        repo.set_misc(IMPORT_MAP_KEY, bytes).await?;
        Ok(())
    }

    /// The version a commit was imported as.
    pub fn get(&self, sha: &GitSha) -> Option<ChangeVersionHash> {
        self.entries.get(sha).copied()
    }

    /// Whether a commit was imported.
    pub fn contains(&self, sha: &GitSha) -> bool {
        self.entries.contains_key(sha)
    }

    /// Record an imported commit.
    pub fn insert(&mut self, sha: GitSha, version: ChangeVersionHash) {
        self.entries.insert(sha, version);
    }

    /// Number of imported commits.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True before anything was imported.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every imported sha.
    pub fn shas(&self) -> impl Iterator<Item = &GitSha> {
        self.entries.keys()
    }
}
