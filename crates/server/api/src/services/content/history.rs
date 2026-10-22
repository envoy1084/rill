//! History at a revision: the versions behind it, a path's history, and
//! blame.

use std::collections::HashMap;

use rill_core::read_content;
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersionHash, FileChange, TrunkSeq};

use super::ContentSvc;
use crate::listing::{Page, Position};
use crate::services::changes::visible;
use crate::services::common::{self, open_repo, path};
use crate::services::{names, revision, Call};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

/// Where a history walk starts: the revision on the first page, the
/// version the previous page ended before on the next.
async fn walk_start(
    ctx: &RepoContext,
    page: &Page,
    revision: &str,
) -> Result<Option<ChangeVersionHash>> {
    match &page.start {
        Position::Start => Ok(revision::resolve(ctx, revision).await?.version),
        Position::After(key) => {
            let bytes: [u8; 32] = key
                .as_slice()
                .try_into()
                .map_err(|_| Error::InvalidPageToken("not a token for this list".into()))?;
            Ok(Some(ChangeVersionHash::from_bytes(bytes)))
        }
        Position::Offset(_) => Err(Error::InvalidPageToken("not a token for this list".into())),
    }
}

/// The token for a walk that continues at `next`.
fn next_token(page: &Page, next: Option<ChangeVersionHash>) -> String {
    next.map(|h| page.token(Position::After(h.as_bytes().to_vec())))
        .unwrap_or_default()
}

impl ContentSvc {
    pub(super) async fn list_history_impl(
        &self,
        request: Call<pb::ListHistoryRequest>,
    ) -> Result<pb::ListHistoryResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let filter = opened.permit().read_any().await?;
        let ctx = &opened.repo;
        let mode = if req.first_parent {
            "first-parent"
        } else {
            "all"
        };
        let page = Page::new(
            req.page_size,
            &req.page_token,
            &[&req.repo, &req.revision, &req.path, mode],
        )?;
        let limit = page.size;
        let Some(start) = walk_start(ctx, &page, &req.revision).await? else {
            return Ok(pb::ListHistoryResponse::default());
        };
        let path_filter = match req.path.as_str() {
            "" => None,
            p => Some(path("path", p)?),
        };
        // The path walk and the first-parent walk both follow first
        // parents, which is what a path history means; the full walk uses
        // the generation-ordered ancestor traversal.
        // The ancestor walk reads each version's info on the way; the
        // first-parent walks leave them to be read below.
        let mut walked: Vec<(ChangeVersionHash, Option<rill_types::ChangeInfo>)> =
            if let Some(p) = &path_filter {
                opened.permit().read(p).await?;
                without_infos(ctx.derive.history(p, &start, limit + 1).await?)
            } else if req.first_parent {
                without_infos(first_parents(ctx, start, limit + 1).await?)
            } else {
                ctx.graph
                    .ancestors(&start, limit + 1)
                    .await?
                    .into_iter()
                    .map(|e| (e.info.hash, Some(e.info)))
                    .collect()
            };
        let next = if walked.len() > limit {
            let overflow = walked[limit].0;
            walked.truncate(limit);
            // Continue from the first parent of the last returned entry in
            // first-parent walks; the ancestor walk is re-entered at the
            // next entry it produced.
            match path_filter.is_some() || req.first_parent {
                true => ctx
                    .derive
                    .change(&walked[limit - 1].0)
                    .await?
                    .first_parent(),
                false => Some(overflow),
            }
        } else {
            None
        };
        let filter = &filter;
        let versions =
            futures::future::try_join_all(walked.into_iter().map(|(hash, info)| async move {
                match info {
                    Some(info) => visible::info(ctx, filter, info).await,
                    None => visible::info_of(ctx, filter, &hash).await,
                }
            }))
            .await?
            .into_iter()
            .flatten()
            .map(|info| common::change_info_proto(ctx, &info))
            .collect();
        Ok(pb::ListHistoryResponse {
            versions,
            next_page_token: next_token(&page, next),
        })
    }

    pub(super) async fn list_path_history_impl(
        &self,
        request: Call<pb::ListPathHistoryRequest>,
    ) -> Result<pb::ListPathHistoryResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        opened.permit().read(&path).await?;
        let ctx = &opened.repo;
        let page = Page::new(
            req.page_size,
            &req.page_token,
            &[&req.repo, &req.revision, &req.path],
        )?;
        let limit = page.size;
        let Some(start) = walk_start(ctx, &page, &req.revision).await? else {
            return Ok(pb::ListPathHistoryResponse::default());
        };
        let mut hashes = ctx.derive.history(&path, &start, limit + 1).await?;
        let next = if hashes.len() > limit {
            hashes.truncate(limit);
            ctx.derive.change(&hashes[limit - 1]).await?.first_parent()
        } else {
            None
        };
        let mut entries = Vec::with_capacity(hashes.len());
        for hash in &hashes {
            let cv = ctx.derive.change(hash).await?;
            // Renames are not tracked: the path is the same at every
            // version. A subtree copy or delete covering the path is
            // reported as the file state it produced.
            let change = match cv.file_changes.get(&path) {
                Some(c) => c.clone(),
                None => {
                    let root = common::root_of(ctx, hash).await?;
                    match rill_core::diff::file_state(&ctx.trees, Some(root), &path).await? {
                        Some((content, kind)) => FileChange::set(content, kind),
                        None => FileChange::Delete,
                    }
                }
            };
            entries.push(pb::PathHistoryEntry {
                version: common::change_info(ctx, hash).await?.into(),
                path: path.to_string(),
                change: pb::FileChange::from(change).into(),
            });
        }
        Ok(pb::ListPathHistoryResponse {
            entries,
            next_page_token: next_token(&page, next),
        })
    }

    pub(super) async fn blame_impl(
        &self,
        request: Call<pb::BlameRequest>,
    ) -> Result<pb::BlameResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        opened.permit().read(&path).await?;
        let ctx = &opened.repo;
        let version = revision::resolve_version(ctx, &req.revision).await?;
        let blame = ctx.derive.blame(&version, &path).await?;
        let root = common::root_of(ctx, &version).await?;
        let state = rill_core::diff::file_state(&ctx.trees, Some(root), &path).await?;
        let bytes = match &state {
            Some((content, _)) => read_content(ctx.repo.objects().as_ref(), content).await?,
            None => return Err(Error::not_found(format!("{path:?} at {version}"))),
        };
        let texts: Vec<&[u8]> = rill_core::diff::split_lines(&bytes).collect();
        let mut seqs: HashMap<ChangeVersionHash, Option<TrunkSeq>> = HashMap::new();
        let mut lines = Vec::with_capacity(blame.len());
        for (i, line) in blame.iter().enumerate() {
            let trunk_seq = match seqs.get(&line.version) {
                Some(s) => *s,
                None => {
                    let s = ctx.repo.trunk_seq_of(&line.version).await?;
                    seqs.insert(line.version, s);
                    s
                }
            };
            lines.push(pb::BlameLine {
                line_no: i as u32 + 1,
                version: names::version(ctx, &line.version),
                author: pb::Principal::from(&line.author).into(),
                trunk_position: trunk_seq.map(TrunkSeq::get),
                text: texts
                    .get(i)
                    .map(|t| bytes::Bytes::copy_from_slice(t))
                    .unwrap_or_default(),
            });
        }
        Ok(pb::BlameResponse { lines })
    }
}

async fn first_parents(
    ctx: &RepoContext,
    start: ChangeVersionHash,
    limit: usize,
) -> Result<Vec<ChangeVersionHash>> {
    let mut out = Vec::new();
    let mut cursor = Some(start);
    while let Some(hash) = cursor {
        if out.len() >= limit {
            break;
        }
        out.push(hash);
        cursor = ctx.graph.parents(&hash).await?.first().copied();
    }
    Ok(out)
}

fn without_infos(
    hashes: Vec<ChangeVersionHash>,
) -> Vec<(ChangeVersionHash, Option<rill_types::ChangeInfo>)> {
    hashes.into_iter().map(|hash| (hash, None)).collect()
}
