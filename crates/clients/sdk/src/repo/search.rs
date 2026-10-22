//! Code search.

use rill_proto::rill::v1 as pb;

use super::RepoClient;
use crate::error::Result;
use crate::types::{GrepQuery, GrepResult, Root};

impl RepoClient {
    /// Regex search over the files of `root`, served from the server's
    /// trigram index. Paths the caller may not read are never returned.
    pub async fn grep(&self, root: &Root, query: &GrepQuery) -> Result<GrepResult> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::GrepRequest {
                repo: self.resource.clone(),
                revision: root.to_string(),
                pattern: query.pattern.clone(),
                case_insensitive: query.case_insensitive,
                path_globs: query.path_globs.clone(),
                max_results: query.max_results,
                context_lines: query.context_lines,
            };
            async move { svc.grep(request).await }
        })
        .await?
        .into_owned();
        GrepResult::from_proto(resp)
    }
}
