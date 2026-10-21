//! Org RPCs, served by the admin service.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_store::OrgConfig;
use tonic::Request;

use crate::auth::caller;
use crate::error::{Error, Result};
use crate::state::AppState;

fn org_proto(org: OrgConfig) -> pb::Org {
    pb::Org {
        name: org.name,
        description: (!org.description.is_empty()).then_some(org.description),
        created_at: timestamp_to_proto(org.created_at),
    }
}

/// Create an org: instance administrators only, since an org is a tenant
/// with its own users, policies and keys.
pub async fn create(
    state: &AppState,
    request: Request<pb::CreateOrgRequest>,
) -> Result<pb::CreateOrgResponse> {
    let ctx = caller(&request)?;
    crate::auth::require_instance_admin(state, &ctx)?;
    let req = request.into_inner();
    let org = state
        .stores
        .create_org(&req.name, req.description.as_deref().unwrap_or(""))
        .await
        .map_err(|e| match e {
            rill_store::Error::Conflict(m) => Error::AlreadyExists(m),
            other => other.into(),
        })?;
    tracing::info!(org = %org.name, by = %ctx.principal, "org created");
    Ok(pb::CreateOrgResponse {
        org: Some(org_proto(org)),
    })
}

/// Every org.
pub async fn list(
    state: &AppState,
    request: Request<pb::ListOrgsRequest>,
) -> Result<pb::ListOrgsResponse> {
    caller(&request)?;
    Ok(pb::ListOrgsResponse {
        orgs: state
            .stores
            .list_orgs()
            .await?
            .into_iter()
            .map(org_proto)
            .collect(),
    })
}
