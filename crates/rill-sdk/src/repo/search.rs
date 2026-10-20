//! Code search.

use rill_proto::rill::v1 as pb;

use super::RepoClient;
use crate::error::Result;
use crate::types::{GrepQuery, GrepResult, Root};

impl RepoClient {
    /// Regex search over the files of `root`, served from the server's
    /// trigram index. Paths the caller may not read are never returned.
    pub async fn grep(&self, root: &Root, query: &GrepQuery) -> Result<GrepResult> {
        let resp = self
            .svc()
            .grep(pb::GrepRequest {
                repo: self.repo.clone(),
                root: Some(root.to_proto()),
                pattern: query.pattern.clone(),
                case_insensitive: query.case_insensitive,
                path_globs: query.path_globs.clone(),
                max_results: query.max_results,
                context_lines: query.context_lines,
            })
            .await?
            .into_inner();
        GrepResult::from_proto(resp)
    }
}
