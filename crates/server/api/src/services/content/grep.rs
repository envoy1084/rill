//! Code search over a revision, served from the trigram index.

use rill_proto::rill::v1 as pb;
use rill_search::SearchQuery;

use super::ContentSvc;
use crate::services::common::{limit, open_repo};
use crate::services::{revision, Call};
use rill_runtime::error::{Error, Result};

const DEFAULT_RESULTS: usize = 5_000;
const MAX_RESULTS: usize = 5_000;
const MAX_CONTEXT: u32 = 20;

impl ContentSvc {
    pub(super) async fn grep_impl(
        &self,
        request: Call<pb::GrepRequest>,
    ) -> Result<pb::GrepResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let filter = opened.permit().read_any().await?;
        if req.pattern.is_empty() {
            return Err(Error::invalid("pattern must not be empty"));
        }
        let ctx = &opened.repo;
        let resolved = revision::resolve(ctx, &req.revision).await?;

        let max = limit(req.max_results, DEFAULT_RESULTS, MAX_RESULTS);
        let query = SearchQuery::new(req.pattern.clone())
            .case_insensitive(req.case_insensitive)
            .path_globs(req.path_globs.iter().map(String::as_str))
            .map_err(|e| Error::invalid(e.to_string()))?
            // Ask for one extra hit so truncation is reported exactly.
            .max_results(max + 1)
            .context_lines(req.context_lines.min(MAX_CONTEXT) as usize);

        let mut hits = ctx.grep(resolved.root, query, filter).await?;
        let truncated = hits.len() > max;
        hits.truncate(max);

        Ok(pb::GrepResponse {
            hits: hits
                .into_iter()
                .map(|h| pb::GrepHit {
                    path: h.path.to_string(),
                    line_no: h.line_no,
                    line: h.line,
                    before: h.before,
                    after: h.after,
                    byte_start: h.byte_range.start as u64,
                    byte_end: h.byte_range.end as u64,
                })
                .collect(),
            truncated,
        })
    }
}
