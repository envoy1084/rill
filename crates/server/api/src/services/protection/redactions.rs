//! Redactions: requested by one org administrator, applied by another.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::names::REDACTION;
use rill_proto::rill::v1 as pb;
use rill_types::identity::{IdentityChange, Redaction};
use rill_types::{ObjectHash, Timestamp};

use super::ProtectionSvc;
use crate::auth::{caller, require_org_admin};
use crate::listing::Page;
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};

fn redaction_proto(r: &Redaction) -> pb::Redaction {
    pb::Redaction {
        name: names::org_resource(&REDACTION, &r.org, &r.id),
        id: r.id.clone(),
        org: r.org.clone(),
        kind: (r.kind as i32).into(),
        hash: bytes::Bytes::copy_from_slice(r.hash.as_bytes()),
        reason: r.reason.clone(),
        requested_by: pb::Principal::from(r.requested_by.clone()).into(),
        requested_at: timestamp_to_proto(r.requested_at),
        approved_by: r.approved_by.clone().map(pb::Principal::from).into(),
        applied_at: r.applied_at.map(timestamp_to_proto),
    }
}

impl ProtectionSvc {
    pub(super) async fn create_redaction_impl(
        &self,
        request: Call<pb::CreateRedactionRequest>,
    ) -> Result<pb::CreateRedactionResponse> {
        let ctx = caller(&request)?;
        let req = request.into_inner();
        let org = names::org(&req.parent)?;
        require_org_admin(&self.state, &ctx, &org).await?;
        let kind = rill_proto::convert::object_kind_from_proto("kind", req.kind)?;
        if req.reason.trim().is_empty() {
            return Err(Error::invalid("a redaction needs a reason"));
        }
        let redaction = Redaction {
            id: format!("red_{}", hex::encode(rand::random::<[u8; 12]>())),
            org,
            kind,
            hash: ObjectHash::from_slice(&req.hash)?,
            reason: req.reason,
            requested_by: ctx.principal.clone(),
            requested_at: Timestamp::now(),
            approved_by: None,
            applied_at: None,
        };
        self.state
            .identity
            .store()
            .request_redaction(&redaction)
            .await?;
        tracing::info!(by = %ctx.principal, org = %redaction.org, id = %redaction.id, hash = %redaction.hash, "redaction requested");
        Ok(pb::CreateRedactionResponse {
            redaction: redaction_proto(&redaction).into(),
        })
    }

    pub(super) async fn approve_redaction_impl(
        &self,
        request: Call<pb::ApproveRedactionRequest>,
    ) -> Result<pb::ApproveRedactionResponse> {
        let ctx = caller(&request)?;
        let (org, id) = names::org_child(&REDACTION, &request.get_ref().name)?;
        require_org_admin(&self.state, &ctx, &org).await?;
        let redaction = self
            .state
            .identity
            .store()
            .approve_redaction(&org, &id, &ctx.principal)
            .await
            .map_err(|e| match e {
                rill_store::Error::Invalid(m) => Error::PermissionDenied(m),
                other => other.into(),
            })?;
        // This server stops serving it now; the others follow the log.
        self.state
            .identity
            .apply(&IdentityChange::RedactionApplied {
                org: org.clone(),
                id: redaction.id.clone(),
                hash: redaction.hash,
            });
        tracing::warn!(by = %ctx.principal, org = %org, id = %redaction.id, hash = %redaction.hash, "redaction applied");
        Ok(pb::ApproveRedactionResponse {
            redaction: redaction_proto(&redaction).into(),
        })
    }

    pub(super) async fn list_redactions_impl(
        &self,
        request: Call<pb::ListRedactionsRequest>,
    ) -> Result<pb::ListRedactionsResponse> {
        let ctx = caller(&request)?;
        let req = request.get_ref();
        let org = names::org(&req.parent)?;
        require_org_admin(&self.state, &ctx, &org).await?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
        let redactions = self.state.identity.store().redactions(&org).await?;
        let (redactions, next_page_token) = page.of(redactions)?;
        Ok(pb::ListRedactionsResponse {
            redactions: redactions.iter().map(redaction_proto).collect(),
            next_page_token,
        })
    }
}
