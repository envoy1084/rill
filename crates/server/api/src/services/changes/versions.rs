//! Reading versions: one, a batch, or a change's history.

use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersionHash, FileChange};

use super::{visible, ChangeSvc};
use crate::grants::GrantBuilder;
use crate::listing::Page;
use crate::services::common::{self, open_repo, Opened};
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};

/// The most versions one BatchGetVersions reads.
pub const MAX_BATCH_VERSIONS: usize = 1000;

impl ChangeSvc {
    pub(super) async fn get_version_impl(
        &self,
        request: Call<pb::GetVersionRequest>,
    ) -> Result<pb::GetVersionResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.name).await?;
        let hash = names::version_in(&opened.repo, &req.name)?;
        let full = req.view == pb::VersionView::Full;
        let version = read_version(&opened, &hash, full)
            .await?
            .ok_or_else(|| Error::not_found(format!("version {hash}")))?;
        Ok(pb::GetVersionResponse {
            version: version.into(),
        })
    }

    pub(super) async fn batch_get_versions_impl(
        &self,
        request: Call<pb::BatchGetVersionsRequest>,
    ) -> Result<pb::BatchGetVersionsResponse> {
        let req = request.get_ref();
        if req.names.len() > MAX_BATCH_VERSIONS {
            return Err(Error::invalid(format!(
                "at most {MAX_BATCH_VERSIONS} versions per batch"
            )));
        }
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        // Refuse a stranger even for an empty batch, which would otherwise
        // confirm that the repository exists.
        opened.permit().read_any().await?;
        let full = req.view == pb::VersionView::Full;
        let mut versions = Vec::with_capacity(req.names.len());
        for name in &req.names {
            let hash = names::version_in(&opened.repo, name)?;
            versions.push(
                read_version(&opened, &hash, full)
                    .await?
                    .ok_or_else(|| Error::not_found(format!("version {hash}")))?,
            );
        }
        Ok(pb::BatchGetVersionsResponse { versions })
    }

    pub(super) async fn list_versions_impl(
        &self,
        request: Call<pb::ListVersionsRequest>,
    ) -> Result<pb::ListVersionsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        let change = names::change_in(&opened.repo, &req.parent)?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
        let filter = opened.permit().read_any().await?;
        let hashes = opened.repo.repo.change_versions(&change).await?;
        let (hashes, next_page_token) = page.of(hashes)?;
        let mut versions = Vec::with_capacity(hashes.len());
        for hash in hashes {
            if let Some(info) = visible::info_of(&opened.repo, &filter, &hash).await? {
                versions.push(common::change_info_proto(&opened.repo, &info));
            }
        }
        Ok(pb::ListVersionsResponse {
            versions,
            next_page_token,
        })
    }
}

/// A version as the caller may see it: its metadata and, when `full`, the
/// changes under paths it may read, signatures and a read grant. `None`
/// when it does not exist or touches nothing the caller may read.
async fn read_version(
    opened: &Opened,
    hash: &ChangeVersionHash,
    full: bool,
) -> Result<Option<pb::Version>> {
    let ctx = &opened.repo;
    let filter = opened.permit().read_any().await?;
    let Some(info) = visible::info_of(ctx, &filter, hash).await? else {
        return Ok(None);
    };
    let info = common::change_info_proto(ctx, &info);
    if !full {
        return Ok(Some(pb::Version {
            info: info.into(),
            ..Default::default()
        }));
    }
    let Some(cv) = ctx.repo.get_change_version(hash).await? else {
        return Ok(None);
    };
    let cv = visible::version(cv, &filter);
    let mut grant = GrantBuilder::new(filter.clone());
    if filter.is_all() {
        grant.version(*hash);
    }
    for (path, change) in &cv.file_changes {
        if let FileChange::Set { content, .. } = change {
            grant.content(content, path);
        }
    }
    let signatures = ctx.repo.get_signatures(hash).await?;
    Ok(Some(pb::Version {
        info: info.into(),
        version: pb::ChangeVersion::from(cv).into(),
        signatures: signatures.into_iter().map(pb::Signature::from).collect(),
        read_grant: grant.sign(opened)?,
    }))
}
