//! Workload issuers an organization trusts.

use rill_proto::rill::v1 as pb;
use rill_types::identity::{IssuerKeys, WorkloadIssuer};
use rill_types::Timestamp;

use crate::auth::{caller, require_org_admin};
use crate::listing::Page;
use crate::services::{names, Call};
use rill_proto::names::WORKLOAD_ISSUER;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

fn spec(i: &WorkloadIssuer) -> pb::WorkloadIssuerSpec {
    pb::WorkloadIssuerSpec {
        resource_name: names::org_resource(&WORKLOAD_ISSUER, &i.org, &i.name),
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

pub async fn update(
    state: &AppState,
    request: Call<pb::UpdateWorkloadIssuerRequest>,
) -> Result<pb::UpdateWorkloadIssuerResponse> {
    let ctx = caller(&request)?;
    let req = request.into_inner();
    let s = req
        .issuer
        .into_option()
        .ok_or(rill_proto::ConvertError::missing("issuer"))?;
    require_org_admin(state, &ctx, &s.org).await?;
    if s.name.is_empty() || s.issuer.is_empty() {
        return Err(Error::invalid("name and issuer are required"));
    }
    let exists = state
        .identity
        .store()
        .workload_issuers(&s.org)
        .await?
        .iter()
        .any(|i| i.name == s.name);
    if !exists && !req.allow_missing {
        return Err(Error::not_found(format!(
            "workload issuer {}; set allow_missing to create it",
            s.name
        )));
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
    Ok(pb::UpdateWorkloadIssuerResponse {
        issuer: spec(&issuer).into(),
    })
}

pub async fn list(
    state: &AppState,
    request: Call<pb::ListWorkloadIssuersRequest>,
) -> Result<pb::ListWorkloadIssuersResponse> {
    let ctx = caller(&request)?;
    let req = request.get_ref();
    let org = names::org(&req.parent)?;
    require_org_admin(state, &ctx, &org).await?;
    let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
    let (issuers, next_page_token) =
        page.of(state.identity.store().workload_issuers(&org).await?)?;
    Ok(pb::ListWorkloadIssuersResponse {
        issuers: issuers.iter().map(spec).collect(),
        next_page_token,
    })
}

pub async fn delete(
    state: &AppState,
    request: Call<pb::DeleteWorkloadIssuerRequest>,
) -> Result<pb::DeleteWorkloadIssuerResponse> {
    let ctx = caller(&request)?;
    let (org, name) = names::org_child(&WORKLOAD_ISSUER, &request.get_ref().name)?;
    require_org_admin(state, &ctx, &org).await?;
    let deleted = state
        .identity
        .store()
        .delete_workload_issuer(&org, &name, &ctx.principal)
        .await?;
    if !deleted {
        return Err(Error::not_found(format!("workload issuer {name}")));
    }
    Ok(pb::DeleteWorkloadIssuerResponse {})
}
