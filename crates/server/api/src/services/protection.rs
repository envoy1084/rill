//! `rill.v1.ProtectionService`: redactions (requested by one org
//! administrator, applied by another) and releasing quarantined versions.

use std::sync::Arc;

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::protection_service_server::ProtectionService;
use rill_types::identity::{IdentityChange, Redaction};
use rill_types::{ObjectHash, ObjectKind, Timestamp};
use tonic::{Request, Response, Status};

use super::common::{hash, open};
use super::idem;
use crate::auth::{caller, require_instance_admin, require_org_admin};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

type GrpcResult<T> = std::result::Result<Response<T>, Status>;

/// The protection service.
#[derive(Clone)]
pub struct ProtectionSvc {
    state: Arc<AppState>,
}

impl ProtectionSvc {
    pub fn new(state: Arc<AppState>) -> Self {
        ProtectionSvc { state }
    }
}

fn org_of(org: &str) -> String {
    if org.is_empty() {
        rill_store::DEFAULT_ORG.to_string()
    } else {
        org.to_string()
    }
}

fn redaction_proto(r: &Redaction) -> pb::RedactionRecord {
    pb::RedactionRecord {
        id: r.id.clone(),
        org: r.org.clone(),
        kind: r.kind as i32,
        hash: bytes::Bytes::copy_from_slice(r.hash.as_bytes()),
        reason: r.reason.clone(),
        requested_by: Some(pb::Principal::from(r.requested_by.clone())),
        requested_at: timestamp_to_proto(r.requested_at),
        approved_by: r.approved_by.clone().map(pb::Principal::from),
        applied_at: r.applied_at.map(timestamp_to_proto),
    }
}

impl ProtectionSvc {
    async fn rotate(
        &self,
        request: Request<pb::RotateDataKeysRequest>,
    ) -> Result<pb::RotateDataKeysResponse> {
        let ctx = caller(&request)?;
        require_instance_admin(&self.state, &ctx)?;
        let keys = self.state.data_keys.as_ref().ok_or_else(|| {
            Error::FailedPrecondition("objects are not encrypted at rest on this server".into())
        })?;
        let r = request.into_inner();
        if r.rewrap {
            let rewrapped = keys.rewrap().await?;
            tracing::info!(by = %ctx.principal, rewrapped, "data keys rewrapped");
            return Ok(pb::RotateDataKeysResponse {
                key_id: None,
                active_from: 0,
                rewrapped: rewrapped as u32,
            });
        }
        let org = org_of(&r.org);
        let now = Timestamp::now();
        let key_id = keys.rotate(&org).await?;
        tracing::info!(by = %ctx.principal, org, key = ?key_id, "data key rotated");
        Ok(pb::RotateDataKeysResponse {
            key_id,
            active_from: timestamp_to_proto(Timestamp::from_millis(
                now.millis() + rill_identity::keys::DATA_KEY_ACTIVATION_MS,
            )),
            rewrapped: 0,
        })
    }

    async fn request(
        &self,
        request: Request<pb::RequestRedactionRequest>,
    ) -> Result<pb::RequestRedactionResponse> {
        let ctx = caller(&request)?;
        let req = request.into_inner();
        let org = org_of(&req.org);
        require_org_admin(&self.state, &ctx, &org).await?;
        let kind = u8::try_from(req.kind)
            .ok()
            .and_then(ObjectKind::from_u8)
            .ok_or_else(|| Error::invalid("kind is required"))?;
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
        Ok(pb::RequestRedactionResponse {
            redaction: Some(redaction_proto(&redaction)),
        })
    }

    async fn approve(
        &self,
        request: Request<pb::ApproveRedactionRequest>,
    ) -> Result<pb::ApproveRedactionResponse> {
        let ctx = caller(&request)?;
        let req = request.into_inner();
        let org = org_of(&req.org);
        require_org_admin(&self.state, &ctx, &org).await?;
        let redaction = self
            .state
            .identity
            .store()
            .approve_redaction(&org, &req.id, &ctx.principal)
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
            redaction: Some(redaction_proto(&redaction)),
        })
    }

    async fn list(
        &self,
        request: Request<pb::ListRedactionsRequest>,
    ) -> Result<pb::ListRedactionsResponse> {
        let ctx = caller(&request)?;
        let org = org_of(&request.get_ref().org);
        require_org_admin(&self.state, &ctx, &org).await?;
        let redactions = self.state.identity.store().redactions(&org).await?;
        Ok(pb::ListRedactionsResponse {
            redactions: redactions.iter().map(redaction_proto).collect(),
        })
    }

    async fn clear(
        &self,
        request: Request<pb::ClearQuarantineRequest>,
    ) -> Result<pb::ClearQuarantineResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().admin().await?;
        let version = hash("version", &request.get_ref().version)?;
        let ctx = &opened.repo;
        if ctx.repo.quarantined(&version).await?.is_none() {
            return Err(Error::not_found(format!("{version} is not quarantined")));
        }
        let log = self.state.stores.identity().log(
            IdentityChange::QuarantineCleared {
                org: ctx.config.org.clone(),
                repo: ctx.config.name.clone(),
                version,
            },
            opened.principal(),
        )?;
        ctx.repo.clear_quarantine(&version, log.to_vec()).await?;
        Ok(pb::ClearQuarantineResponse {})
    }
}

#[tonic::async_trait]
impl ProtectionService for ProtectionSvc {
    async fn request_redaction(
        &self,
        request: Request<pb::RequestRedactionRequest>,
    ) -> GrpcResult<pb::RequestRedactionResponse> {
        idem::once(&self.state, request, |r| async move {
            self.request(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn approve_redaction(
        &self,
        request: Request<pb::ApproveRedactionRequest>,
    ) -> GrpcResult<pb::ApproveRedactionResponse> {
        idem::once(&self.state, request, |r| async move {
            self.approve(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn list_redactions(
        &self,
        request: Request<pb::ListRedactionsRequest>,
    ) -> GrpcResult<pb::ListRedactionsResponse> {
        self.list(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn clear_quarantine(
        &self,
        request: Request<pb::ClearQuarantineRequest>,
    ) -> GrpcResult<pb::ClearQuarantineResponse> {
        idem::once(&self.state, request, |r| async move {
            self.clear(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn rotate_data_keys(
        &self,
        request: Request<pb::RotateDataKeysRequest>,
    ) -> GrpcResult<pb::RotateDataKeysResponse> {
        idem::once(&self.state, request, |r| async move {
            self.rotate(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }
}
