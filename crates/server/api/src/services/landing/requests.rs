//! Asking for changes to land and reading the requests back.

use rill_lander::LandRequest;
use rill_proto::rill::v1 as pb;
use std::sync::Arc;

use rill_lander::Lander;
use rill_types::{
    ChangeId, ChangeVersionHash, GroupMode, Line, RefName, RepoPathBuf, ReviewState, ReviewStatus,
    RiskClass,
};

use super::LandingSvc;
use crate::listing::Page;
use crate::services::budget;
use crate::services::common::{self, open_repo, Opened};
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};

impl LandingSvc {
    pub(super) async fn request_land_impl(
        &self,
        request: Call<pb::RequestLandRequest>,
    ) -> Result<pb::RequestLandResponse> {
        let (opened, id) = self.change(&request, &request.get_ref().change).await?;
        opened.permit().land().await?;
        let state = opened.repo.review().get(&id).await?;
        let version = landable_version(&opened, &state).await?;
        let (line, lander) = line_and_lander(&opened, &request.get_ref().onto).await?;
        budget::spend_now(&opened, budget::LANDS).await?;
        let ticket = lander
            .enqueue(land_request(&opened, id, version, state.risk).onto(line))
            .await?;
        Ok(pb::RequestLandResponse {
            land_request: common::land_request(&opened.repo, ticket).await?.into(),
        })
    }

    pub(super) async fn request_land_stack_impl(
        &self,
        request: Call<pb::RequestLandStackRequest>,
    ) -> Result<pb::RequestLandStackResponse> {
        let (opened, id) = self.change(&request, &request.get_ref().change).await?;
        opened.permit().land().await?;
        let (line, lander) = line_and_lander(&opened, &request.get_ref().onto).await?;
        let review = opened.repo.review();
        let stack = review.stack_of(&id).await?;
        let top = stack.iter().position(|c| *c == id).unwrap_or(0);
        let mut states = Vec::with_capacity(top + 1);
        for member in &stack[..=top] {
            let state = review.get(member).await?;
            if state.status != ReviewStatus::Landed {
                states.push(state);
            }
        }
        if states.is_empty() {
            return Err(Error::precondition(format!(
                "every change in the stack up to {id} already landed"
            )));
        }
        let mut requests = Vec::with_capacity(states.len());
        let mut below: Option<ChangeVersionHash> = None;
        for state in &states {
            let version = landable_version(&opened, state).await?;
            if let Some(below) = below {
                let cv = opened.repo.derive.change(&version).await?;
                if cv.first_parent() != Some(below) {
                    return Err(Error::precondition(format!(
                        "change {} is not based on the current version of the change \
                         below it; sync the stack first",
                        state.change_id
                    )));
                }
            }
            below = Some(version);
            requests.push(
                land_request(&opened, state.change_id, version, state.risk).onto(line.clone()),
            );
        }
        for _ in &requests {
            budget::spend_now(&opened, budget::LANDS).await?;
        }
        let mode = match request.get_ref().prefix {
            true => GroupMode::Prefix,
            false => GroupMode::All,
        };
        let tickets = lander.enqueue_group(requests, mode).await?;
        let mut out = Vec::with_capacity(tickets.len());
        for ticket in tickets {
            out.push(common::land_request(&opened.repo, ticket).await?);
        }
        Ok(pb::RequestLandStackResponse { land_requests: out })
    }

    pub(super) async fn get_land_request_impl(
        &self,
        request: Call<pb::GetLandRequestRequest>,
    ) -> Result<pb::GetLandRequestResponse> {
        let name = &request.get_ref().name;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let ticket = names::ticket_in(&opened.repo, name)?;
        Ok(pb::GetLandRequestResponse {
            land_request: common::land_request(&opened.repo, ticket).await?.into(),
        })
    }

    pub(super) async fn list_land_requests_impl(
        &self,
        request: Call<pb::ListLandRequestsRequest>,
    ) -> Result<pb::ListLandRequestsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
        let pending = opened.repo.lander.queue().pending(usize::MAX).await?;
        let (pending, next_page_token) = page.of(pending)?;
        let mut land_requests = Vec::with_capacity(pending.len());
        for queued in pending {
            land_requests.push(common::land_request(&opened.repo, queued.ticket).await?);
        }
        Ok(pb::ListLandRequestsResponse {
            land_requests,
            next_page_token,
        })
    }

    /// Open the repository a change name is in, and the change's id.
    async fn change<T>(&self, request: &Call<T>, name: &str) -> Result<(Opened, ChangeId)> {
        let opened = open_repo(&self.state, request, name).await?;
        let id = names::change_in(&opened.repo, name)?;
        Ok((opened, id))
    }
}

/// The line `onto` names ("trunk" when absent) and its lander. A release
/// must have been cut.
async fn line_and_lander(opened: &Opened, onto: &str) -> Result<(Line, Arc<Lander>)> {
    let line = match onto {
        "" | "trunk" => Line::Trunk,
        name => RefName::new(name)
            .ok()
            .and_then(|r| Line::of_ref(&r))
            .ok_or_else(|| {
                Error::invalid(format!(
                    "onto is \"trunk\" or \"release/<name>\", not {name:?}"
                ))
            })?,
    };
    match &line {
        Line::Trunk => Ok((line, opened.repo.lander.clone())),
        Line::Release(name) => {
            if opened.repo.repo.line_head(&line).await?.is_none() {
                return Err(Error::not_found(format!("{line} was never cut")));
            }
            let lander = opened.repo.releases.get(name);
            Ok((line, lander))
        }
    }
}

/// The version of `state` that may be asked to land by the caller now.
async fn landable_version(opened: &Opened, state: &ReviewState) -> Result<ChangeVersionHash> {
    let id = state.change_id;
    match state.status {
        ReviewStatus::Landed => {
            return Err(Error::precondition(format!("change {id} already landed")))
        }
        ReviewStatus::Landing => {
            return Err(Error::precondition(format!(
                "change {id} is already landing"
            )))
        }
        ReviewStatus::Abandoned => {
            return Err(Error::precondition(format!("change {id} is abandoned")))
        }
        _ => {}
    }
    let decision = opened
        .repo
        .review()
        .evaluate_for(state, opened.principal())
        .await?;
    if !decision.allowed {
        return Err(Error::PolicyDenied {
            message: format!("submit policy does not allow landing {id}"),
            missing: decision.reasons,
        });
    }
    state
        .current_version()
        .ok_or_else(|| Error::precondition(format!("change {id} has no versions")))
}

fn land_request(
    opened: &Opened,
    id: ChangeId,
    version: ChangeVersionHash,
    risk: RiskClass,
) -> LandRequest {
    LandRequest::new(id, version, opened.principal().clone(), risk)
        .sponsored_by(opened.ctx.claims.as_ref().and_then(|c| c.sponsor.clone()))
}
