//! Trunk health: its state, the incidents behind it, and freezing it.

use rill_proto::convert::{hash_to_proto, id_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{Incident, RepoPathBuf, Resolution, TrunkHealth};

use super::TrunkSvc;
use crate::listing::{FieldValue, Filter, Kind, Page, Record};
use crate::services::common::{open_repo, Opened};
use crate::services::{names, Call};
use rill_runtime::error::Result;
use rill_runtime::state::RepoContext;

impl TrunkSvc {
    pub(super) async fn get_trunk_health_impl(
        &self,
        request: Call<pb::GetTrunkHealthRequest>,
    ) -> Result<pb::GetTrunkHealthResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let health = opened.repo.repo.trunk_health().await?;
        let open = opened.repo.lander.open_incidents().await?;
        Ok(pb::GetTrunkHealthResponse {
            health: health_to_proto(&health).into(),
            open: open
                .iter()
                .map(|i| incident_to_proto(&opened.repo, i))
                .collect(),
        })
    }

    pub(super) async fn list_incidents_impl(
        &self,
        request: Call<pb::ListIncidentsRequest>,
    ) -> Result<pb::ListIncidentsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let filter = Filter::parse(&req.filter, &[("state", Kind::Str)])?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.filter])?;
        let mut incidents = opened.repo.repo.incidents(usize::MAX).await?;
        incidents.retain(|i| filter.matches(&Listed(i)));
        let (incidents, next_page_token) = page.of(incidents)?;
        Ok(pb::ListIncidentsResponse {
            incidents: incidents
                .iter()
                .map(|i| incident_to_proto(&opened.repo, i))
                .collect(),
            next_page_token,
        })
    }

    pub(super) async fn freeze_trunk_impl(
        &self,
        request: Call<pb::FreezeTrunkRequest>,
    ) -> Result<pb::FreezeTrunkResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        let reason = Some(request.get_ref().reason.clone()).filter(|r| !r.is_empty());
        Ok(pb::FreezeTrunkResponse {
            health: set_frozen(&opened, true, reason).await?.into(),
        })
    }

    pub(super) async fn unfreeze_trunk_impl(
        &self,
        request: Call<pb::UnfreezeTrunkRequest>,
    ) -> Result<pb::UnfreezeTrunkResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        Ok(pb::UnfreezeTrunkResponse {
            health: set_frozen(&opened, false, None).await?.into(),
        })
    }
}

async fn set_frozen(
    opened: &Opened,
    frozen: bool,
    reason: Option<String>,
) -> Result<pb::TrunkHealth> {
    opened.permit().admin().await?;
    let health = opened
        .repo
        .lander
        .set_frozen(frozen, reason, opened.principal())
        .await?;
    Ok(health_to_proto(&health))
}

/// An incident as a list filter sees it.
struct Listed<'a>(&'a Incident);

impl Record for Listed<'_> {
    fn field(&self, field: &str) -> FieldValue {
        match field {
            "state" => FieldValue::Str(self.0.state.as_str().into()),
            _ => FieldValue::Missing,
        }
    }
}

fn health_to_proto(health: &TrunkHealth) -> pb::TrunkHealth {
    pb::TrunkHealth {
        state: health.state.as_str().into(),
        since: timestamp_to_proto(health.since),
        open: health.open.iter().map(|s| s.get()).collect(),
        failing_steps: health.failing_steps.clone(),
        frozen: health.frozen,
        frozen_reason: health.frozen_reason.clone(),
    }
}

fn incident_to_proto(ctx: &RepoContext, incident: &Incident) -> pb::Incident {
    let (resolution, resolved_at) = match incident.resolution {
        Some(Resolution::Reverted(at)) => (Some("reverted"), Some(at.get())),
        Some(Resolution::Fixed(at)) => (Some("fixed"), Some(at.get())),
        Some(Resolution::Recovered(at)) => (Some("recovered"), Some(at.get())),
        None => (None, None),
    };
    pb::Incident {
        name: names::incident(ctx, incident.culprit),
        culprit: incident.culprit.get(),
        version: hash_to_proto(&incident.version),
        change_id: id_to_proto(&incident.change),
        failures: incident.failures.clone(),
        steps: incident.steps.clone(),
        details: incident.details.clone(),
        author: pb::Principal::from(incident.author.clone()).into(),
        session: incident.session.clone(),
        model: incident.model.clone(),
        sponsor: incident.sponsor.clone().map(pb::Principal::from).into(),
        state: incident.state.as_str().into(),
        opened_at: timestamp_to_proto(incident.opened_at),
        updated_at: timestamp_to_proto(incident.updated_at),
        fix: incident.fix.map(|s| s.get()),
        resolution: resolution.map(str::to_string),
        resolved_at,
        revert_blocked: incident.revert_blocked.clone(),
        assigned: incident.assigned.clone(),
    }
}
