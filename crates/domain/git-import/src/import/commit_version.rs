//! Translating one git commit into a change version.

use std::collections::BTreeMap;

use bytes::Bytes;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, FileChange, Principal, Provenance, RepoPathBuf,
};

use super::importer::Importer;
use super::target::ImportTarget;
use crate::error::{Error, Result};
use crate::reader::{CommitChanges, GitChange, GitCommit};

/// Key under which the git sha is stored in `ChangeVersion::extra`.
pub const GIT_SHA_EXTRA: &str = "git_sha";

/// Provenance keys keeping the git author and commit time.
pub const GIT_AUTHOR_PROVENANCE: &str = "git_author";
pub const GIT_DATE_PROVENANCE: &str = "git_date";

/// Tool name recorded in provenance.
pub(super) const TOOL: &str = "rill-git-import";

impl<T: ImportTarget> Importer<T> {
    pub(super) async fn import_commit(
        &self,
        commit: &GitCommit,
        changes: CommitChanges,
    ) -> Result<ChangeVersionHash> {
        let mut author = Principal::human(if commit.author_name.is_empty() {
            commit.author_email.clone()
        } else {
            commit.author_name.clone()
        });
        author.identity = Some(commit.author_email.clone());
        let mut provenance = Provenance {
            tool: Some(TOOL.to_string()),
            ..Default::default()
        };
        // A server stamps the importer as the author; the git author and
        // date stay with the version.
        provenance.extra.insert(
            GIT_AUTHOR_PROVENANCE.to_string(),
            format!("{} <{}>", commit.author_name, commit.author_email),
        );
        provenance.extra.insert(
            GIT_DATE_PROVENANCE.to_string(),
            commit.committed_at.millis().to_string(),
        );
        let mut parents = Vec::with_capacity(commit.parents.len());
        for parent in &commit.parents {
            parents.push(self.mapped(&commit.sha, parent)?);
        }
        let mut file_changes = BTreeMap::new();
        for change in changes.changes {
            let path = change.path().to_string();
            let repo_path =
                RepoPathBuf::new(&path).map_err(|e| Error::InvalidPath(path.clone(), e))?;
            let file_change = match change {
                GitChange::Set { content, kind, .. } => {
                    FileChange::set(self.repo.store_content(content).await?, kind)
                }
                GitChange::Delete { .. } => FileChange::Delete,
            };
            file_changes.insert(repo_path, file_change);
        }
        let mut extra = BTreeMap::new();
        extra.insert(
            GIT_SHA_EXTRA.to_string(),
            Bytes::from(commit.sha.to_bytes()),
        );
        let cv = ChangeVersion {
            change_id: ChangeId::derived("git", commit.sha.as_str()),
            parents,
            predecessor: None,
            subtree_changes: BTreeMap::new(),
            file_changes,
            author,
            provenance,
            intent: None,
            message: commit.message.clone(),
            created_at: commit.committed_at,
            extra,
        };
        let hash = self.repo.store_version(&cv).await?;
        tracing::debug!(sha = %commit.sha, version = %hash, files = cv.file_changes.len(), "imported commit");
        Ok(hash)
    }
}
