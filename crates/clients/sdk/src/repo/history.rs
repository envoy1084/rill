//! History queries: versions, log, path history and blame.

use rill_proto::convert::required;
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
            let svc = self.changes();
            let request = pb::GetVersionRequest {
                name: crate::names::version(&self.resource, &hash),
                view: pb::VersionView::Full.into(),
            };
            async move { svc.get_version(request).await }
        })
        .await?
        .into_owned();
        let found = required("get_version.version", resp.version)?;
        let version: ChangeVersion = required("version.version", found.version)?.try_into()?;
        self.grants().record(
            &found.read_grant,
            version.file_changes.values().filter_map(|c| match c {
                rill_types::FileChange::Set { content, .. } => crate::grants::content_hash(content),
                _ => None,
            }),
        );
        let signatures = found
            .signatures
            .into_iter()
            .map(Signature::try_from)
            .collect::<std::result::Result<_, _>>()?;
        Ok((version, signatures))
    }

    /// Metadata of a change version.
    pub async fn change_info(&self, hash: ChangeVersionHash) -> Result<ChangeInfo> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.changes();
            let request = pb::GetVersionRequest {
                name: crate::names::version(&self.resource, &hash),
                view: pb::VersionView::Basic.into(),
            };
            async move { svc.get_version(request).await }
        })
        .await?
        .into_owned();
        let found = required("get_version.version", resp.version)?;
        Ok(required("version.info", found.info)?.try_into()?)
    }

    /// Every version of a change in creation order; the last is current.
    pub async fn change_versions(&self, change_id: ChangeId) -> Result<Vec<ChangeInfo>> {
        let mut versions = Vec::new();
        let mut page_token = String::new();
        loop {
            let resp = crate::client::retry_idempotent(|| {
                let svc = self.changes();
                let request = pb::ListVersionsRequest {
                    parent: crate::names::change(&self.resource, &change_id),
                    page_size: 1000,
                    page_token: page_token.clone(),
                };
                async move { svc.list_versions(request).await }
            })
            .await?
            .into_owned();
            versions.extend(convert_infos(resp.versions)?);
            if resp.next_page_token.is_empty() {
                return Ok(versions);
            }
            page_token = resp.next_page_token;
        }
    }

    /// The current version of a change.
    pub async fn current_version(&self, change_id: ChangeId) -> Result<ChangeInfo> {
        self.change_versions(change_id).await?.pop().ok_or_else(|| {
            crate::Error::status(
                connectrpc::ErrorCode::NotFound,
                format!("change {change_id} has no versions"),
            )
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
        Ok(self
            .log_page(start, limit, path, false, None)
            .await?
            .entries)
    }

    /// One page of [`RepoClient::log`], with first-parent control; pass the
    /// returned `next_page_token` with the same arguments to continue.
    pub async fn log_page(
        &self,
        start: LogStart,
        limit: u32,
        path: Option<&RepoPath>,
        first_parent: bool,
        page_token: Option<&str>,
    ) -> Result<LogPage> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::ListHistoryRequest {
                repo: self.resource.clone(),
                revision: start.to_root().to_string(),
                path: path.map(|p| p.to_string()).unwrap_or_default(),
                first_parent,
                page_size: limit as i32,
                page_token: page_token.unwrap_or_default().to_string(),
            };
            async move { svc.list_history(request).await }
        })
        .await?
        .into_owned();
        Ok(LogPage {
            entries: convert_infos(resp.versions)?,
            next_page_token: (!resp.next_page_token.is_empty()).then_some(resp.next_page_token),
        })
    }

    /// Versions that touched `path`; pass the returned `next_page_token`
    /// with the same arguments to continue.
    pub async fn path_history(
        &self,
        path: &RepoPath,
        start: LogStart,
        limit: u32,
        page_token: Option<&str>,
    ) -> Result<PathHistoryPage> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::ListPathHistoryRequest {
                repo: self.resource.clone(),
                revision: start.to_root().to_string(),
                path: path.to_string(),
                page_size: limit as i32,
                page_token: page_token.unwrap_or_default().to_string(),
            };
            async move { svc.list_path_history(request).await }
        })
        .await?
        .into_owned();
        Ok(PathHistoryPage {
            entries: resp
                .entries
                .into_iter()
                .map(PathHistoryEntry::try_from)
                .collect::<Result<_>>()?,
            next_page_token: (!resp.next_page_token.is_empty()).then_some(resp.next_page_token),
        })
    }

    /// Line-by-line attribution of `path` at `hash`.
    pub async fn blame(&self, hash: ChangeVersionHash, path: &RepoPath) -> Result<Vec<BlameLine>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::BlameRequest {
                repo: self.resource.clone(),
                revision: hash.to_hex(),
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
