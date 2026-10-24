//! Code search over a revision, served from the trigram index: the shards
//! this server serves are searched here, the rest on the peers that serve
//! them (or here, when a peer does not answer).

use rill_proto::rill::v1 as pb;
use rill_search::{assemble, SearchHit, SearchQuery, ShardResults, ShardSelect};

use super::peers::file_to_proto;
use super::ContentSvc;
use crate::services::common::{limit, open_repo};
use crate::services::{revision, Call};
use rill_runtime::error::{Error, Result};

const DEFAULT_RESULTS: usize = 5_000;
const MAX_RESULTS: usize = 5_000;
const MAX_CONTEXT: u32 = 20;
/// How much longer than the time budget a peer is given to answer.
const PEER_SLACK: std::time::Duration = std::time::Duration::from_secs(5);

impl ContentSvc {
    pub(super) async fn grep_impl(
        &self,
        request: Call<pb::GrepRequest>,
    ) -> Result<pb::GrepResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let filter = opened.permit().read_any().await?;
        let ctx = &opened.repo;
        let resolved = revision::resolve(ctx, &req.revision).await?;
        let (query, max) = self.query(req)?;
        let scope = ctx
            .search_scope(
                resolved.version,
                resolved.root,
                req.min_seq.map(rill_types::TrunkSeq),
            )
            .await?;

        let plan = ctx.search_nodes.plan();
        let pinned = pb::GrepRequest {
            revision: match resolved.version {
                Some(v) => v.to_string(),
                None => format!("tree:{}", resolved.root),
            },
            min_seq: None,
            ..req.clone()
        };
        let authorization = request.headers().get(http::header::AUTHORIZATION);
        let timeout = query.time_budget + PEER_SLACK;
        let local = ctx.grep_shards(
            scope.clone(),
            query.clone(),
            filter.clone(),
            ShardSelect {
                shards: plan.local,
                live: true,
            },
        );
        let remote = futures::future::try_join_all(plan.remote.into_iter().map(|(url, shards)| {
            let (scope, query, filter, pinned) =
                (scope.clone(), query.clone(), filter.clone(), pinned.clone());
            async move {
                match self
                    .peers
                    .grep_shards(&url, authorization, pinned, &shards, timeout)
                    .await
                {
                    Ok(found) => Ok(found),
                    Err(e) => {
                        tracing::warn!(peer = %url, error = %e, "search peer failed; searching its shards here");
                        let select = ShardSelect {
                            shards: Some(shards),
                            live: false,
                        };
                        ctx.grep_shards(scope, query, filter, select).await
                    }
                }
            }
        }));
        let (local, remote) = tokio::join!(local, remote);
        let mut found = local?;
        for part in remote? {
            found.files.extend(part.files);
            found.capped |= part.capped;
            found.partial |= part.partial;
        }

        let result = assemble(found.files, query.max_results, found.capped, found.partial);
        let mut hits = result.hits;
        let truncated = result.truncated || hits.len() > max;
        hits.truncate(max);
        Ok(pb::GrepResponse {
            indexed_through: ctx.search.through().map_or(0, |t| t.seq.0),
            files_matched: result.files,
            partial: result.partial,
            directories: result.facets.directories.into_iter().collect(),
            languages: result.facets.languages.into_iter().collect(),
            hits: hits.into_iter().map(hit_to_proto).collect(),
            truncated,
        })
    }

    pub(super) async fn grep_shards_impl(
        &self,
        request: Call<pb::GrepShardsRequest>,
    ) -> Result<pb::GrepShardsResponse> {
        let req = request.get_ref();
        let grep = req
            .grep
            .as_option()
            .ok_or_else(|| Error::invalid("grep is required"))?;
        let opened = open_repo(&self.state, &request, &grep.repo).await?;
        let filter = opened.permit().read_any().await?;
        let ctx = &opened.repo;
        let shards = self.state.config.search.shards.max(1) as u32;
        if let Some(bad) = req.shards.iter().find(|&&s| s >= shards) {
            return Err(Error::invalid(format!(
                "shard {bad} is out of range (0..{shards})"
            )));
        }
        let resolved = revision::resolve(ctx, &grep.revision).await?;
        let (query, _) = self.query(grep)?;
        let scope = ctx
            .search_scope(resolved.version, resolved.root, None)
            .await?;
        let select = ShardSelect {
            shards: Some(req.shards.iter().map(|&s| s as usize).collect()),
            live: false,
        };
        let ShardResults {
            files,
            capped,
            partial,
        } = ctx.grep_shards(scope, query, filter, select).await?;
        Ok(pb::GrepShardsResponse {
            files: files.into_iter().map(file_to_proto).collect(),
            capped,
            partial,
        })
    }

    /// The query a request asks for, and the most hits it returns.
    fn query(&self, req: &pb::GrepRequest) -> Result<(SearchQuery, usize)> {
        if req.pattern.is_empty() {
            return Err(Error::invalid("pattern must not be empty"));
        }
        let max = limit(req.max_results, DEFAULT_RESULTS, MAX_RESULTS);
        let budget = std::time::Duration::from_millis(self.state.config.search.time_budget_ms);
        let query = SearchQuery::new(req.pattern.clone())
            .case_insensitive(req.case_insensitive)
            .literal(req.literal)
            .structural(req.structural)
            .languages(req.languages.clone())
            .time_budget(budget)
            .path_globs(req.path_globs.iter().map(String::as_str))
            .map_err(|e| Error::invalid(e.to_string()))?
            // Ask for one extra hit so truncation is reported exactly.
            .max_results(max + 1)
            .context_lines(req.context_lines.min(MAX_CONTEXT) as usize);
        Ok((query, max))
    }
}

/// A hit as the wire carries it.
pub(super) fn hit_to_proto(h: SearchHit) -> pb::GrepHit {
    pb::GrepHit {
        path: h.path.to_string(),
        line_no: h.line_no,
        line: h.line,
        before: h.before,
        after: h.after,
        byte_start: h.byte_range.start as u64,
        byte_end: h.byte_range.end as u64,
    }
}
