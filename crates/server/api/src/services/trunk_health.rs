//! Trunk health on `LanderService`: its state, the incidents behind it,
//! and freezing it.

use rill_lander::healing::Selector;
use rill_proto::convert::{hash_to_proto, id_from_proto, id_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{Incident, Resolution, TrunkHealth, TrunkSeq};
use tonic::Request;

use super::common::open;
use super::lander::LanderSvc;
use rill_runtime::error::{Error, Result};

/// Incidents in a page by default.
const DEFAULT_INCIDENTS: usize = 20;
const MAX_INCIDENTS: usize = 1000;

impl LanderSvc {
    pub(super) async fn trunk_health_impl(
        &self,
        request: Request<pb::GetTrunkHealthRequest>,
    ) -> Result<pb::GetTrunkHealthResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened
            .permit()
            .read(&rill_types::RepoPathBuf::root())
            .await?;
        let health = opened.repo.repo.trunk_health().await?;
        let open = opened.repo.lander.open_incidents().await?;
        Ok(pb::GetTrunkHealthResponse {
            health: Some(health_to_proto(&health)),
            open: open.iter().map(incident_to_proto).collect(),
        })
    }

    pub(super) async fn incidents_impl(
        &self,
        request: Request<pb::ListIncidentsRequest>,
    ) -> Result<pb::ListIncidentsResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened
            .permit()
            .read(&rill_types::RepoPathBuf::root())
            .await?;
        let limit = match request.get_ref().limit as usize {
            0 => DEFAULT_INCIDENTS,
            n => n.min(MAX_INCIDENTS),
        };
        let incidents = opened.repo.repo.incidents(limit).await?;
        Ok(pb::ListIncidentsResponse {
            incidents: incidents.iter().map(incident_to_proto).collect(),
        })
    }

    pub(super) async fn freeze_impl(
        &self,
        request: Request<pb::FreezeTrunkRequest>,
    ) -> Result<pb::FreezeTrunkResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().admin().await?;
        let req = request.get_ref();
        let reason = (!req.reason.is_empty()).then(|| req.reason.clone());
        let health = opened
            .repo
            .lander
            .set_frozen(req.frozen, reason, opened.principal())
            .await?;
        Ok(pb::FreezeTrunkResponse {
            health: Some(health_to_proto(&health)),
        })
    }
}

impl LanderSvc {
    pub(super) async fn revert_set_impl(
        &self,
        request: Request<pb::RevertSetRequest>,
    ) -> Result<pb::RevertSetResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().admin().await?;
        let req = request.get_ref();
        let selector = Selector {
            session: req.session.clone(),
            model: req.model.clone(),
            sponsor: req.sponsor.clone(),
            author: req.author.clone(),
            intent: req
                .intent
                .as_ref()
                .map(|i| id_from_proto("intent", i))
                .transpose()?,
            since: req.since.map(TrunkSeq),
            until: req.until.map(TrunkSeq),
        };
        if !selector.names_origin() {
            return Err(Error::invalid(
                "name a session, model, sponsor, author or intent to revert",
            ));
        }
        let lander = &opened.repo.lander;
        let entries = if req.apply {
            let reason = match req.reason.is_empty() {
                true => format!("mass revert of {}", selector.describe()),
                false => req.reason.clone(),
            };
            lander
                .apply_reverts(&selector, &reason, opened.principal())
                .await?
                .into_iter()
                .map(|(seq, ticket)| pb::RevertSetEntry {
                    seq: seq.get(),
                    ticket: Some(ticket.0.to_string()),
                    ..Default::default()
                })
                .collect()
        } else {
            lander
                .plan_reverts(&selector)
                .await?
                .into_iter()
                .map(|e| pb::RevertSetEntry {
                    seq: e.seq.get(),
                    change_id: id_to_proto(&e.change),
                    summary: e.summary,
                    conflicts: e.conflicts.iter().map(ToString::to_string).collect(),
                    ticket: None,
                })
                .collect()
        };
        Ok(pb::RevertSetResponse { entries })
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

fn incident_to_proto(incident: &Incident) -> pb::Incident {
    let (resolution, resolved_at) = match incident.resolution {
        Some(Resolution::Reverted(at)) => (Some("reverted"), Some(at.get())),
        Some(Resolution::Fixed(at)) => (Some("fixed"), Some(at.get())),
        Some(Resolution::Recovered(at)) => (Some("recovered"), Some(at.get())),
        None => (None, None),
    };
    pb::Incident {
        culprit: incident.culprit.get(),
        version: hash_to_proto(&incident.version),
        change_id: id_to_proto(&incident.change),
        failures: incident.failures.clone(),
        steps: incident.steps.clone(),
        details: incident.details.clone(),
        author: Some(incident.author.clone().into()),
        session: incident.session.clone(),
        model: incident.model.clone(),
        sponsor: incident.sponsor.clone().map(Into::into),
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
