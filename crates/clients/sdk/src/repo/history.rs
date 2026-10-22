//! History queries: versions, log, path history and blame.

use rill_proto::convert::{hash_to_proto, id_to_proto, opt_hash_from_proto, required};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, ChangeInfo, ChangeVersion, ChangeVersionHash, RepoPath, Signature};

use super::RepoClient;
use crate::error::Result;
use crate::types::{BlameLine, LogPage, LogStart, PathHistoryEntry, PathHistoryPage};

impl RepoClient {
    /// A change version by hash, without its signatures.
    pub async fn change_version(&self, hash: ChangeVersionHash) -> Result<ChangeVersion> {
        Ok(self.change_version_signed(hash).await?.0)
    }

    /// A change version by hash, with its detached signatures.
    pub async fn change_version_signed(
        &self,
        hash: ChangeVersionHash,
    ) -> Result<(ChangeVersion, Vec<Signature>)> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetChangeVersionRequest {
                repo: self.repo.clone(),
                hash: hash_to_proto(&hash),
            };
            async move { svc.get_change_version(request).await }
        })
        .await?
        .into_owned();
        let version: ChangeVersion =
            required("get_change_version.version", resp.version)?.try_into()?;
        self.grants().record(
            &resp.read_grant,
            version.file_changes.values().filter_map(|c| match c {
                rill_types::FileChange::Set { content, .. } => crate::grants::content_hash(content),
                _ => None,
            }),
        );
        let signatures = resp
            .signatures
            .into_iter()
            .map(Signature::try_from)
            .collect::<std::result::Result<_, _>>()?;
        Ok((version, signatures))
    }

    /// Metadata of a change version.
    pub async fn change_info(&self, hash: ChangeVersionHash) -> Result<ChangeInfo> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetChangeInfoRequest {
                repo: self.repo.clone(),
                hash: hash_to_proto(&hash),
            };
            async move { svc.get_change_info(request).await }
        })
        .await?
        .into_owned();
        Ok(required("get_change_info.info", resp.info)?.try_into()?)
    }

    /// Every version of a change in creation order; the last is current.
    pub async fn change_versions(&self, change_id: ChangeId) -> Result<Vec<ChangeInfo>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetChangeVersionsRequest {
                repo: self.repo.clone(),
                change_id: id_to_proto(&change_id),
            };
            async move { svc.get_change_versions(request).await }
        })
        .await?
        .into_owned();
        convert_infos(resp.versions)
    }

    /// The current version of a change.
    pub async fn current_version(&self, change_id: ChangeId) -> Result<ChangeInfo> {
        self.change_versions(change_id)
            .await?
            .pop()
            .ok_or_else(|| crate::Error::Status {
                code: connectrpc::ErrorCode::NotFound,
                message: format!("change {change_id} has no versions"),
            })
    }

    /// Walk history from `start`, newest first, optionally restricted to
    /// versions touching `path`.
    pub async fn log(
        &self,
        start: LogStart,
        limit: u32,
        path: Option<&RepoPath>,
    ) -> Result<Vec<ChangeInfo>> {
        Ok(self.log_page(start, limit, path, false).await?.entries)
    }

    /// [`RepoClient::log`] with the continuation hash and first-parent
    /// control.
    pub async fn log_page(
        &self,
        start: LogStart,
        limit: u32,
        path: Option<&RepoPath>,
        first_parent: bool,
    ) -> Result<LogPage> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::LogRequest {
                repo: self.repo.clone(),
                start: start.to_proto().into(),
                limit,
                path: path.map(|p| p.to_string()),
                first_parent,
            };
            async move { svc.log(request).await }
        })
        .await?
        .into_owned();
        Ok(LogPage {
            entries: convert_infos(resp.entries)?,
            next_start: opt_hash_from_proto("log.next_start", resp.next_start.as_ref())?,
        })
    }

    /// Versions that touched `path`, following renames.
    pub async fn path_history(
        &self,
        path: &RepoPath,
        start: LogStart,
        limit: u32,
    ) -> Result<PathHistoryPage> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::PathHistoryRequest {
                repo: self.repo.clone(),
                path: path.to_string(),
                start: start.to_proto().into(),
                limit,
            };
            async move { svc.path_history(request).await }
        })
        .await?
        .into_owned();
        Ok(PathHistoryPage {
            entries: resp
                .entries
                .into_iter()
                .map(PathHistoryEntry::try_from)
                .collect::<Result<_>>()?,
            next_start: opt_hash_from_proto("path_history.next_start", resp.next_start.as_ref())?,
        })
    }

    /// Line-by-line attribution of `path` at `hash`.
    pub async fn blame(&self, hash: ChangeVersionHash, path: &RepoPath) -> Result<Vec<BlameLine>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::BlameRequest {
                repo: self.repo.clone(),
                hash: hash_to_proto(&hash),
                path: path.to_string(),
            };
            async move { svc.blame(request).await }
        })
        .await?
        .into_owned();
        resp.lines.into_iter().map(BlameLine::try_from).collect()
    }
}

fn convert_infos(infos: Vec<pb::ChangeInfo>) -> Result<Vec<ChangeInfo>> {
    infos
        .into_iter()
        .map(|i| ChangeInfo::try_from(i).map_err(Into::into))
        .collect()
}
