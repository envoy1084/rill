//! History RPCs: change versions, log, path history and blame.

use std::collections::HashMap;

use rill_core::read_content;
use rill_proto::convert::{hash_to_proto, id_from_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, ChangeVersionHash, FileChange, TrunkSeq};

use super::RepoSvc;
use crate::error::{Error, Result};
use crate::services::common::{self, hash, limit, open, path, resolve_version};

/// Default and maximum page sizes for history walks.
const DEFAULT_LOG_LIMIT: usize = 100;
const MAX_LOG_LIMIT: usize = 10_000;

impl RepoSvc {
    pub(super) async fn get_change_version_impl(
        &self,
        request: tonic::Request<pb::GetChangeVersionRequest>,
    ) -> Result<pb::GetChangeVersionResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        opened
            .permit()
            .read(&rill_types::RepoPathBuf::root())
            .await?;
        let hash = hash("hash", &req.hash)?;
        let cv = opened
            .repo
            .repo
            .get_change_version(&hash)
            .await?
            .ok_or_else(|| Error::not_found(format!("change version {hash}")))?;
        let signatures = opened.repo.repo.get_signatures(&hash).await?;
        Ok(pb::GetChangeVersionResponse {
            version: Some(pb::ChangeVersion::from(cv)),
            signatures: signatures.into_iter().map(pb::Signature::from).collect(),
        })
    }

    pub(super) async fn get_change_info_impl(
        &self,
        request: tonic::Request<pb::GetChangeInfoRequest>,
    ) -> Result<pb::GetChangeInfoResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        opened
            .permit()
            .read(&rill_types::RepoPathBuf::root())
            .await?;
        let hash = hash("hash", &req.hash)?;
        Ok(pb::GetChangeInfoResponse {
            info: Some(common::change_info(&opened.repo, &hash).await?),
        })
    }

    pub(super) async fn get_change_versions_impl(
        &self,
        request: tonic::Request<pb::GetChangeVersionsRequest>,
    ) -> Result<pb::GetChangeVersionsResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        opened
            .permit()
            .read(&rill_types::RepoPathBuf::root())
            .await?;
        let change_id: ChangeId = id_from_proto("change_id", &req.change_id)?;
        let mut versions = Vec::new();
        for hash in opened.repo.repo.change_versions(&change_id).await? {
            versions.push(common::change_info(&opened.repo, &hash).await?);
        }
        Ok(pb::GetChangeVersionsResponse { versions })
    }

    pub(super) async fn log_impl(
        &self,
        request: tonic::Request<pb::LogRequest>,
    ) -> Result<pb::LogResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        opened
            .permit()
            .read(&rill_types::RepoPathBuf::root())
            .await?;
        let ctx = &opened.repo;
        let limit = limit(req.limit, DEFAULT_LOG_LIMIT, MAX_LOG_LIMIT);
        let Some(start) = resolve_version(ctx, req.start.as_ref()).await? else {
            return Ok(pb::LogResponse {
                entries: Vec::new(),
                next_start: None,
            });
        };
        let path_filter = match &req.path {
            Some(p) if !p.is_empty() => Some(path("path", p)?),
            _ => None,
        };
        // The path walk and the first-parent walk both follow first
        // parents, which is what a path history means; the full walk uses
        // the generation-ordered ancestor traversal.
        let mut hashes: Vec<ChangeVersionHash> = if let Some(p) = &path_filter {
            opened.permit().read(p).await?;
            ctx.derive.history(p, &start, limit + 1).await?
        } else if req.first_parent {
            first_parents(ctx, start, limit + 1).await?
        } else {
            ctx.graph
                .ancestors(&start, limit + 1)
                .await?
                .into_iter()
                .map(|e| e.info.hash)
                .collect()
        };
        let next_start = if hashes.len() > limit {
            hashes.truncate(limit);
            // Continue from the first parent of the last returned entry in
            // first-parent walks; the ancestor walk is re-entered at the
            // next entry it would have produced.
            match path_filter.is_some() || req.first_parent {
                true => ctx
                    .derive
                    .change(&hashes[limit - 1])
                    .await?
                    .first_parent()
                    .map(|h| hash_to_proto(&h)),
                false => ctx
                    .graph
                    .ancestors(&start, limit + 1)
                    .await?
                    .get(limit)
                    .map(|e| hash_to_proto(&e.info.hash)),
            }
        } else {
            None
        };
        let mut entries = Vec::with_capacity(hashes.len());
        for hash in &hashes {
            entries.push(common::change_info(ctx, hash).await?);
        }
        Ok(pb::LogResponse {
            entries,
            next_start,
        })
    }

    pub(super) async fn path_history_impl(
        &self,
        request: tonic::Request<pb::PathHistoryRequest>,
    ) -> Result<pb::PathHistoryResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        opened.permit().read(&path).await?;
        let ctx = &opened.repo;
        let limit = limit(req.limit, DEFAULT_LOG_LIMIT, MAX_LOG_LIMIT);
        let Some(start) = resolve_version(ctx, req.start.as_ref()).await? else {
            return Ok(pb::PathHistoryResponse {
                entries: Vec::new(),
                next_start: None,
            });
        };
        let mut hashes = ctx.derive.history(&path, &start, limit + 1).await?;
        let next_start = if hashes.len() > limit {
            hashes.truncate(limit);
            ctx.derive
                .change(&hashes[limit - 1])
                .await?
                .first_parent()
                .map(|h| hash_to_proto(&h))
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
                info: Some(common::change_info(ctx, hash).await?),
                path: path.to_string(),
                change: Some(pb::FileChange::from(change)),
            });
        }
        Ok(pb::PathHistoryResponse {
            entries,
            next_start,
        })
    }

    pub(super) async fn blame_impl(
        &self,
        request: tonic::Request<pb::BlameRequest>,
    ) -> Result<pb::BlameResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        opened.permit().read(&path).await?;
        let ctx = &opened.repo;
        let version = hash("hash", &req.hash)?;
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
                version: hash_to_proto(&line.version),
                author: Some(pb::Principal::from(&line.author)),
                trunk_seq: trunk_seq.map(TrunkSeq::get),
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
    ctx: &crate::state::RepoContext,
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
