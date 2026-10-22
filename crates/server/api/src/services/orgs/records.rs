//! Creating, reading and listing organizations.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_store::OrgConfig;

use super::OrgSvc;
use crate::auth::caller;
use crate::listing::Page;
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};

fn org_proto(org: OrgConfig) -> pb::Org {
    pb::Org {
        name: rill_proto::names::ORG.format(&[&org.name]),
        org_id: org.name,
        description: (!org.description.is_empty()).then_some(org.description),
        created_at: timestamp_to_proto(org.created_at),
    }
}

impl OrgSvc {
    /// Create an org: instance administrators only, since an org is a
    /// tenant with its own users, policies and keys.
    pub(super) async fn create_org_impl(
        &self,
        request: Call<pb::CreateOrgRequest>,
    ) -> Result<pb::CreateOrgResponse> {
        let state = &self.state;
        let ctx = caller(&request)?;
        crate::auth::require_instance_admin(state, &ctx)?;
        let req = request.into_inner();
        let org = state
            .stores
            .create_org_by(
                &req.org_id,
                req.description.as_deref().unwrap_or(""),
                &ctx.principal,
            )
            .await
            .map_err(|e| match e {
                rill_store::Error::Conflict(m) => Error::AlreadyExists(m),
                other => other.into(),
            })?;
        tracing::info!(org = %org.name, by = %ctx.principal, "org created");
        Ok(pb::CreateOrgResponse {
            org: org_proto(org).into(),
        })
    }

    pub(super) async fn get_org_impl(
        &self,
        request: Call<pb::GetOrgRequest>,
    ) -> Result<pb::GetOrgResponse> {
        caller(&request)?;
        let id = names::org(&request.get_ref().name)?;
        let org = self
            .state
            .stores
            .list_orgs()
            .await?
            .into_iter()
            .find(|o| o.name == id)
            .ok_or_else(|| Error::not_found(format!("org {id}")))?;
        Ok(pb::GetOrgResponse {
            org: org_proto(org).into(),
        })
    }

    pub(super) async fn list_orgs_impl(
        &self,
        request: Call<pb::ListOrgsRequest>,
    ) -> Result<pb::ListOrgsResponse> {
        caller(&request)?;
        let req = request.get_ref();
        let page = Page::new(req.page_size, &req.page_token, &[])?;
        let (orgs, next_page_token) = page.of(self.state.stores.list_orgs().await?)?;
        Ok(pb::ListOrgsResponse {
            orgs: orgs.into_iter().map(org_proto).collect(),
            next_page_token,
        })
    }
}
