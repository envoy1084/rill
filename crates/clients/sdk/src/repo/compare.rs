//! Comparison: diffs, merge bases and ancestry.

use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersionHash, RepoPathBuf};

use super::RepoClient;
use crate::error::Result;
use crate::types::{DiffFormat, DiffOptions, DiffResult, FileDiff, Root};

impl RepoClient {
    /// Diff `target` against `base`, restricted to `paths` (empty means
    /// everything), with three lines of context.
    pub async fn diff(
        &self,
        base: &Root,
        target: &Root,
        paths: Vec<RepoPathBuf>,
        format: DiffFormat,
    ) -> Result<Vec<FileDiff>> {
        let opts = DiffOptions {
            paths,
            format,
            ..DiffOptions::default()
        };
        Ok(self.diff_with(base, target, opts).await?.files)
    }

    /// [`RepoClient::diff`] with full control and the truncation flag.
    pub async fn diff_with(
        &self,
        base: &Root,
        target: &Root,
        opts: DiffOptions,
    ) -> Result<DiffResult> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::DiffRequest {
                repo: self.resource.clone(),
                base: base.to_string(),
                target: target.to_string(),
                paths: opts.paths.iter().map(|p| p.to_string()).collect(),
                format: opts.format.to_proto().into(),
                context_lines: opts.context_lines,
                max_files: opts.limit,
            };
            async move { svc.diff(request).await }
        })
        .await?
        .into_owned();
        let files: Vec<FileDiff> = resp
            .files
            .into_iter()
            .map(FileDiff::try_from)
            .collect::<Result<_>>()?;
        self.grants().record(
            &resp.read_grant,
            files
                .iter()
                .flat_map(|f| [&f.before, &f.after])
                .flatten()
                .filter_map(|(content, _)| crate::grants::content_hash(content)),
        );
        Ok(DiffResult {
            files,
            truncated: resp.truncated,
        })
    }

    /// Common ancestors of two versions; empty when they share no history.
    pub async fn merge_base(
        &self,
        a: ChangeVersionHash,
        b: ChangeVersionHash,
    ) -> Result<Vec<ChangeVersionHash>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::FindMergeBaseRequest {
                repo: self.resource.clone(),
                a: a.to_hex(),
                b: b.to_hex(),
            };
            async move { svc.find_merge_base(request).await }
        })
        .await?
        .into_owned();
        resp.bases
            .iter()
            .map(|name| crate::names::version_hash(name))
            .collect()
    }

    /// True when `ancestor` is reachable from `descendant`.
    pub async fn is_ancestor(
        &self,
        ancestor: ChangeVersionHash,
        descendant: ChangeVersionHash,
    ) -> Result<bool> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::IsAncestorRequest {
                repo: self.resource.clone(),
                ancestor: ancestor.to_hex(),
                descendant: descendant.to_hex(),
            };
            async move { svc.is_ancestor(request).await }
        })
        .await?
        .into_owned();
        Ok(resp.is_ancestor)
    }
}
