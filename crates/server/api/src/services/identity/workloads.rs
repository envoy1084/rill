//! Workload issuers an organization trusts.

use rill_proto::rill::v1 as pb;
use rill_types::identity::{IssuerKeys, WorkloadIssuer};
use rill_types::Timestamp;
use tonic::Request;

use crate::auth::{caller, require_org_admin};
use crate::error::{Error, Result};
use crate::state::AppState;

fn spec(i: &WorkloadIssuer) -> pb::WorkloadIssuerSpec {
    pb::WorkloadIssuerSpec {
        org: i.org.clone(),
        name: i.name.clone(),
        issuer: i.issuer.clone(),
        keys: Some(match &i.keys {
            IssuerKeys::Discovery => pb::workload_issuer_spec::Keys::Discovery(true),
            IssuerKeys::Uri(uri) => pb::workload_issuer_spec::Keys::JwksUri(uri.clone()),
            IssuerKeys::Inline(doc) => pb::workload_issuer_spec::Keys::Jwks(doc.clone()),
        }),
        audiences: i.audiences.clone(),
    }
}

pub async fn put(
    state: &AppState,
    request: Request<pb::PutWorkloadIssuerRequest>,
) -> Result<pb::PutWorkloadIssuerResponse> {
    let ctx = caller(&request)?;
    let s = request
        .into_inner()
        .issuer
        .ok_or_else(|| Error::invalid("issuer is required"))?;
    require_org_admin(state, &ctx, &s.org).await?;
    if s.name.is_empty() || s.issuer.is_empty() {
        return Err(Error::invalid("name and issuer are required"));
    }
    if s.audiences.is_empty() {
        return Err(Error::invalid(
            "name the audiences tokens must be minted for",
        ));
    }
    let keys = match s.keys {
        None | Some(pb::workload_issuer_spec::Keys::Discovery(_)) => IssuerKeys::Discovery,
        Some(pb::workload_issuer_spec::Keys::JwksUri(uri)) => IssuerKeys::Uri(uri),
        Some(pb::workload_issuer_spec::Keys::Jwks(doc)) => {
            serde_json::from_str::<serde_json::Value>(&doc)
                .map_err(|e| Error::invalid(format!("jwks: {e}")))?;
            IssuerKeys::Inline(doc)
        }
    };
    let issuer = WorkloadIssuer {
        org: s.org,
        name: s.name,
        issuer: s.issuer,
        keys,
        audiences: s.audiences,
        created_at: Timestamp::now(),
    };
    state
        .identity
        .store()
        .put_workload_issuer(&issuer, &ctx.principal)
        .await?;
    tracing::info!(by = %ctx.principal, org = %issuer.org, issuer = %issuer.issuer, "workload issuer trusted");
    Ok(pb::PutWorkloadIssuerResponse {
        issuer: Some(spec(&issuer)),
    })
}

pub async fn list(
    state: &AppState,
    request: Request<pb::ListWorkloadIssuersRequest>,
) -> Result<pb::ListWorkloadIssuersResponse> {
    let ctx = caller(&request)?;
    let org = request.into_inner().org;
    require_org_admin(state, &ctx, &org).await?;
    let issuers = state.identity.store().workload_issuers(&org).await?;
    Ok(pb::ListWorkloadIssuersResponse {
        issuers: issuers.iter().map(spec).collect(),
    })
}

pub async fn delete(
    state: &AppState,
    request: Request<pb::DeleteWorkloadIssuerRequest>,
) -> Result<pb::DeleteWorkloadIssuerResponse> {
    let ctx = caller(&request)?;
    let req = request.into_inner();
    require_org_admin(state, &ctx, &req.org).await?;
    let deleted = state
        .identity
        .store()
        .delete_workload_issuer(&req.org, &req.name, &ctx.principal)
        .await?;
    Ok(pb::DeleteWorkloadIssuerResponse { deleted })
}
