//! Trunk's head and queue, and its landings page by page.

use bytes::Bytes;
use rill_proto::convert::hash_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{RepoPathBuf, TrunkSeq};

use super::verification::verification;
use super::TrunkSvc;
use crate::listing::{Page, Position};
use crate::services::common::{self, open_repo};
use crate::services::Call;
use rill_runtime::error::{Error, Result};

impl TrunkSvc {
    pub(super) async fn get_trunk_status_impl(
        &self,
        request: Call<pb::GetTrunkStatusRequest>,
    ) -> Result<pb::GetTrunkStatusResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let ctx = &opened.repo;
        let (head_position, head_hash) = match ctx.repo.trunk_head().await? {
            Some((s, h)) => (s.get(), hash_to_proto(&h)),
            None => (0, Bytes::new()),
        };
        let mut in_flight = Vec::new();
        for queued in ctx
            .lander
            .queue()
            .pending(crate::listing::page::MAX_PAGE_SIZE)
            .await?
        {
            in_flight.push(common::land_request_proto(ctx, &queued.record()));
        }
        let last_verified_position = match &ctx.postland {
            Some(postland) => postland
                .recent()
                .await?
                .iter()
                .filter(|r| r.is_green())
                .map(|r| r.seq.get())
                .max(),
            None => None,
        };
        Ok(pb::GetTrunkStatusResponse {
            head_position,
            head_hash,
            in_flight,
            last_verified_position,
        })
    }

    pub(super) async fn list_landings_impl(
        &self,
        request: Call<pb::ListLandingsRequest>,
    ) -> Result<pb::ListLandingsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let ctx = &opened.repo;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
        let Some((head, _)) = ctx.repo.trunk_head().await? else {
            return Ok(pb::ListLandingsResponse::default());
        };
        // Newest first: a page starting at offset k covers head-k down.
        let skipped = match &page.start {
            Position::Start => 0,
            Position::Offset(n) => *n,
            Position::After(_) => {
                return Err(Error::InvalidPageToken("not a token for this list".into()))
            }
        };
        let to = TrunkSeq(head.get().saturating_sub(skipped));
        let from = TrunkSeq(to.get().saturating_sub(page.size as u64));
        let mut landings = Vec::new();
        let mut verifications = Vec::new();
        for (seq, hash) in ctx.repo.trunk_range(from, to).await?.into_iter().rev() {
            landings.push(common::landing(ctx, seq, hash).await?);
            verifications.push(verification(ctx, seq).await?);
        }
        let next_page_token = match from.get() {
            0 => String::new(),
            _ => page.token(Position::Offset(skipped + (to.get() - from.get()))),
        };
        Ok(pb::ListLandingsResponse {
            landings,
            verifications,
            next_page_token,
        })
    }
}
