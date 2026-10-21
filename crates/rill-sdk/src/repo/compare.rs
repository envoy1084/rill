//! Comparison: diffs, merge bases and ancestry.

use rill_proto::convert::{hash_to_proto, hashes_from_proto};
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
            let mut svc = self.svc();
            let request = pb::DiffRequest {
                repo: self.repo.clone(),
                base: Some(base.to_proto()),
                target: Some(target.to_proto()),
                paths: opts.paths.iter().map(|p| p.to_string()).collect(),
                format: opts.format.to_proto(),
                context_lines: opts.context_lines,
                limit: opts.limit,
            };
            async move { svc.diff(request).await }
        })
        .await?
        .into_inner();
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
            let mut svc = self.svc();
            let request = pb::MergeBaseRequest {
                repo: self.repo.clone(),
                a: hash_to_proto(&a),
                b: hash_to_proto(&b),
            };
            async move { svc.merge_base(request).await }
        })
        .await?
        .into_inner();
        Ok(hashes_from_proto("merge_base.bases", &resp.bases)?)
    }

    /// True when `ancestor` is reachable from `descendant`.
    pub async fn is_ancestor(
        &self,
        ancestor: ChangeVersionHash,
        descendant: ChangeVersionHash,
    ) -> Result<bool> {
        let resp = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::IsAncestorRequest {
                repo: self.repo.clone(),
                ancestor: hash_to_proto(&ancestor),
                descendant: hash_to_proto(&descendant),
            };
            async move { svc.is_ancestor(request).await }
        })
        .await?
        .into_inner();
        Ok(resp.is_ancestor)
    }
}
