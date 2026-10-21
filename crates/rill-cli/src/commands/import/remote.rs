//! Importing into a running server through its API: content and versions
//! go through the normal write RPCs and trunk through `AppendTrunk`; the
//! sha map is kept in a local file next to the git repository.

use std::path::PathBuf;

use async_trait::async_trait;
use bytes::Bytes;
use rill_git_import::{Error, ImportMap, ImportTarget, Result};
use rill_sdk::Client;
use rill_types::{ChangeVersion, ChangeVersionHash, ContentRef, TrunkSeq};

/// A repository on a server, imported into through the SDK.
pub struct RemoteTarget {
    client: Client,
    repo: String,
    map_file: PathBuf,
}

impl RemoteTarget {
    pub fn new(client: Client, repo: String, map_file: PathBuf) -> Self {
        RemoteTarget {
            client,
            repo,
            map_file,
        }
    }
}

fn remote(e: rill_sdk::Error) -> Error {
    Error::Target(e.to_string())
}

#[async_trait]
impl ImportTarget for RemoteTarget {
    async fn store_content(&self, bytes: Bytes) -> Result<ContentRef> {
        self.client
            .repo(&self.repo)
            .store_content(bytes)
            .await
            .map_err(remote)
    }

    async fn store_version(&self, version: &ChangeVersion) -> Result<ChangeVersionHash> {
        self.client
            .repo(&self.repo)
            .create_change_version(version, Vec::new())
            .await
            .map_err(remote)
    }

    async fn trunk_head(&self) -> Result<Option<(TrunkSeq, ChangeVersionHash)>> {
        match self.client.repo(&self.repo).trunk_head().await {
            Ok(head) => Ok(head),
            // A new repository's trunk has no head yet.
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(remote(e)),
        }
    }

    async fn trunk_seq_of(&self, version: &ChangeVersionHash) -> Result<Option<TrunkSeq>> {
        let info = self
            .client
            .repo(&self.repo)
            .change_info(*version)
            .await
            .map_err(remote)?;
        Ok(info.trunk_seq)
    }

    async fn append_trunk(
        &self,
        expected: Option<ChangeVersionHash>,
        version: ChangeVersionHash,
    ) -> Result<TrunkSeq> {
        self.client
            .admin()
            .append_trunk(&self.repo, expected, version)
            .await
            .map_err(remote)
    }

    async fn load_map(&self) -> Result<ImportMap> {
        match tokio::fs::read(&self.map_file).await {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| Error::Codec(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ImportMap::new()),
            Err(e) => Err(Error::Target(format!("{}: {e}", self.map_file.display()))),
        }
    }

    async fn save_map(&self, map: &ImportMap) -> Result<()> {
        let bytes = serde_json::to_vec(map).map_err(|e| Error::Codec(e.to_string()))?;
        let partial = self.map_file.with_extension("partial");
        tokio::fs::write(&partial, bytes)
            .await
            .and(tokio::fs::rename(&partial, &self.map_file).await)
            .map_err(|e| Error::Target(format!("{}: {e}", self.map_file.display())))
    }
}
