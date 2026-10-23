//! `GetOwners` and `ExplainOwnership`: trunk's owner sets for a path, and
//! where a change stands with each set it touches.

use rill_policy::owners::{OwnerSet, SetCoverage};
use rill_proto::rill::v1 as pb;
use rill_review::lifecycle::notes;
use rill_runtime::workers::escalation::ESCALATION_NOTE;
use rill_types::RepoPathBuf;

use super::ReviewSvc;
use crate::services::common::open_repo;
use crate::services::Call;
use rill_runtime::error::{Error, Result};

fn set_proto(set: OwnerSet) -> pb::OwnerSet {
    pb::OwnerSet {
        id: set.id,
        dir: set.dir.into_string(),
        owners: set.owners,
        required: set.required,
        sensitive: set.sensitive,
        sla_ms: set.sla.map(|d| d.as_millis() as i64),
        escalate_to: set.escalate_to,
    }
}

impl ReviewSvc {
    pub(super) async fn get_owners_impl(
        &self,
        request: Call<pb::GetOwnersRequest>,
    ) -> Result<pb::GetOwnersResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let path = RepoPathBuf::new(&req.path).map_err(|e| Error::invalid(format!("path: {e}")))?;
        if path.is_root() {
            return Err(Error::invalid("path is required"));
        }
        opened.permit().read(&path).await?;
        let owners = opened.repo.review().trunk_owners().await?;
        Ok(pb::GetOwnersResponse {
            sets: owners.sets_for(&path).into_iter().map(set_proto).collect(),
        })
    }

    pub(super) async fn explain_ownership_impl(
        &self,
        request: Call<pb::ExplainOwnershipRequest>,
    ) -> Result<pb::ExplainOwnershipResponse> {
        let (opened, id) = self.change(&request, &request.get_ref().change).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let review = opened.repo.review();
        let state = review.get(&id).await?;
        let coverage = review.owner_coverage(&state).await?;
        let covered = coverage.iter().all(|c| c.covered);
        let escalation = |set: &str| {
            notes(&state, ESCALATION_NOTE)
                .find(|(id, _)| *id == set)
                .and_then(|(_, note)| {
                    let mut parts = note.splitn(3, '|');
                    let level = parts.next()?.parse::<u32>().ok()?;
                    Some((level, parts.next()?.to_string()))
                })
        };
        let sets = coverage
            .into_iter()
            .map(
                |SetCoverage {
                     set,
                     approvals,
                     covered,
                 }| {
                    let escalated = escalation(&set.id);
                    pb::OwnerSetStatus {
                        escalations: escalated.as_ref().map_or(0, |(level, _)| *level),
                        escalated_to: escalated.map(|(_, to)| to),
                        set: set_proto(set).into(),
                        approvals,
                        covered,
                    }
                },
            )
            .collect();
        Ok(pb::ExplainOwnershipResponse { sets, covered })
    }
}
