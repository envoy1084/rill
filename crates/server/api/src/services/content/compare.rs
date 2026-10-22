//! Comparing revisions: diffs, merge bases and ancestry.

use rill_proto::rill::v1 as pb;
use rill_types::{ContentRef, RepoPathBuf};

use super::ContentSvc;
use crate::grants::GrantBuilder;
use crate::services::common::open_repo;
use crate::services::diff::{file_diffs, DiffOptions};
use crate::services::{names, revision, Call};
use rill_runtime::error::Result;

impl ContentSvc {
    pub(super) async fn diff_impl(
        &self,
        request: Call<pb::DiffRequest>,
    ) -> Result<pb::DiffResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let filter = opened.permit().read_any().await?;
        let ctx = &opened.repo;
        let base = revision::resolve(ctx, &req.base).await?;
        let target = revision::resolve(ctx, &req.target).await?;
        let target_cv = match target.version {
            Some(v) => Some(ctx.derive.change(&v).await?),
            None => None,
        };
        let options = DiffOptions::new(
            &req.paths,
            req.format,
            req.context_lines,
            req.max_files,
            filter.clone(),
        )?;
        let (files, truncated) = file_diffs(
            ctx,
            Some(base.root),
            Some(target.root),
            target_cv.as_ref(),
            &options,
        )
        .await?;
        let mut grant = GrantBuilder::new(filter);
        grant_diffs(&mut grant, &files);
        Ok(pb::DiffResponse {
            files,
            truncated,
            read_grant: grant.sign(&opened)?,
        })
    }

    pub(super) async fn find_merge_base_impl(
        &self,
        request: Call<pb::FindMergeBaseRequest>,
    ) -> Result<pb::FindMergeBaseResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        opened.permit().read_any().await?;
        let ctx = &opened.repo;
        let a = revision::resolve_version(ctx, &req.a).await?;
        let b = revision::resolve_version(ctx, &req.b).await?;
        let base = ctx.graph.merge_base(&a, &b).await?;
        Ok(pb::FindMergeBaseResponse {
            bases: base
                .map(|h| vec![names::version(ctx, &h)])
                .unwrap_or_default(),
        })
    }

    pub(super) async fn is_ancestor_impl(
        &self,
        request: Call<pb::IsAncestorRequest>,
    ) -> Result<pb::IsAncestorResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        opened.permit().read_any().await?;
        let ancestor = revision::resolve_version(&opened.repo, &req.ancestor).await?;
        let descendant = revision::resolve_version(&opened.repo, &req.descendant).await?;
        Ok(pb::IsAncestorResponse {
            is_ancestor: opened
                .repo
                .graph
                .is_ancestor(&ancestor, &descendant)
                .await?,
        })
    }
}

/// Grant the content on both sides of each file diff.
pub(crate) fn grant_diffs(grant: &mut GrantBuilder, files: &[pb::FileDiff]) {
    for file in files {
        let Ok(path) = RepoPathBuf::new(file.path.as_str()) else {
            continue;
        };
        for state in [&file.before, &file.after]
            .into_iter()
            .filter_map(|s| s.as_option())
        {
            if let Some(content) = state
                .content
                .clone()
                .into_option()
                .and_then(|c| ContentRef::try_from(c).ok())
            {
                grant.content(&content, &path);
            }
        }
    }
}
