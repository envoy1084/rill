//! `rill.v1.AuditService`: reading, sealing and following an org's audit
//! chain.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::Stream;
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::audit_service_server::AuditService;
use rill_types::audit::{AuditEntry, AuditOutcome, AuditRoot, INSTANCE_AUDIT};
use tonic::{Request, Response, Status};

use crate::auth::{caller, require_instance_admin, require_org_admin};
use crate::error::{Error, Result};
use crate::state::AppState;

type GrpcResult<T> = std::result::Result<Response<T>, Status>;

const DEFAULT_LIMIT: usize = 500;
const MAX_LIMIT: usize = 5000;
/// How often a watch looks for newly chained entries.
const WATCH_POLL: Duration = Duration::from_millis(500);

/// The audit service.
#[derive(Clone)]
pub struct AuditSvc {
    state: Arc<AppState>,
}

impl AuditSvc {
    pub fn new(state: Arc<AppState>) -> Self {
        AuditSvc { state }
    }
}

/// The org a request names, once the caller may read its audit log.
async fn authorized<T>(state: &AppState, request: &Request<T>, org: &str) -> Result<String> {
    let ctx = caller(request)?;
    let org = if org.is_empty() {
        rill_store::DEFAULT_ORG.to_string()
    } else {
        org.to_string()
    };
    if org == INSTANCE_AUDIT {
        require_instance_admin(state, &ctx)?;
    } else {
        require_org_admin(state, &ctx, &org).await?;
    }
    Ok(org)
}

fn encode<T: serde::Serialize>(value: &T) -> Result<bytes::Bytes> {
    postcard::to_allocvec(value)
        .map(bytes::Bytes::from)
        .map_err(|e| Error::internal(e.to_string()))
}

fn entry_proto(entry: &AuditEntry) -> Result<pb::AuditEntryRecord> {
    let r = &entry.record;
    let (allowed, denied_reason) = match &r.outcome {
        AuditOutcome::Allowed => (true, None),
        AuditOutcome::Denied { reason } => (false, Some(reason.clone())),
    };
    Ok(pb::AuditEntryRecord {
        seq: entry.seq,
        org: r.org.clone(),
        at: timestamp_to_proto(r.at),
        actor: Some(pb::Principal::from(r.actor.clone())),
        action: r.action.clone(),
        target: r.target.clone(),
        allowed,
        denied_reason,
        detail: r.detail.clone().into_iter().collect(),
        hash: bytes::Bytes::copy_from_slice(&entry.hash()),
        prev: bytes::Bytes::copy_from_slice(&entry.prev),
        encoded: encode(entry)?,
    })
}

fn root_proto(root: &AuditRoot) -> Result<pb::AuditRootRecord> {
    Ok(pb::AuditRootRecord {
        first_seq: root.first_seq,
        last_seq: root.last_seq,
        head: bytes::Bytes::copy_from_slice(&root.head),
        export: bytes::Bytes::copy_from_slice(root.export.as_bytes()),
        sealed_at: timestamp_to_proto(root.sealed_at),
        key_id: root.key_id,
        encoded: encode(root)?,
    })
}

type EntryStream =
    Pin<Box<dyn Stream<Item = std::result::Result<pb::AuditEntryRecord, Status>> + Send>>;

#[tonic::async_trait]
impl AuditService for AuditSvc {
    async fn list_audit_entries(
        &self,
        request: Request<pb::ListAuditEntriesRequest>,
    ) -> GrpcResult<pb::ListAuditEntriesResponse> {
        let result: Result<_> = async {
            let req = request.get_ref();
            let org = authorized(&self.state, &request, &req.org).await?;
            let limit = match req.limit as usize {
                0 => DEFAULT_LIMIT,
                n => n.min(MAX_LIMIT),
            };
            let audit = self.state.stores.audit();
            let entries = audit
                .entries(&org, req.after_seq, limit)
                .await?
                .iter()
                .map(entry_proto)
                .collect::<Result<_>>()?;
            Ok(pb::ListAuditEntriesResponse {
                entries,
                head_seq: audit.head(&org).await?.seq,
            })
        }
        .await;
        result.map(Response::new).map_err(Status::from)
    }

    async fn list_audit_roots(
        &self,
        request: Request<pb::ListAuditRootsRequest>,
    ) -> GrpcResult<pb::ListAuditRootsResponse> {
        let result: Result<_> = async {
            let org = authorized(&self.state, &request, &request.get_ref().org).await?;
            let roots = self
                .state
                .stores
                .audit()
                .roots(&org)
                .await?
                .iter()
                .map(root_proto)
                .collect::<Result<_>>()?;
            Ok(pb::ListAuditRootsResponse { roots })
        }
        .await;
        result.map(Response::new).map_err(Status::from)
    }

    async fn seal_audit(
        &self,
        request: Request<pb::SealAuditRequest>,
    ) -> GrpcResult<pb::SealAuditResponse> {
        let result: Result<_> = async {
            let org = authorized(&self.state, &request, &request.get_ref().org).await?;
            crate::audit::worker::catch_up(&self.state).await?;
            let root = crate::audit::seal::seal(&self.state, &org).await?;
            Ok(pb::SealAuditResponse {
                root: root.as_ref().map(root_proto).transpose()?,
            })
        }
        .await;
        result.map(Response::new).map_err(Status::from)
    }

    async fn list_audit_keys(
        &self,
        request: Request<pb::ListAuditKeysRequest>,
    ) -> GrpcResult<pb::ListAuditKeysResponse> {
        caller(&request)?;
        let keys = self
            .state
            .identity
            .audit_keys()
            .into_iter()
            .map(|(id, key, _)| pb::AuditKey {
                id,
                public_key: key.into(),
            })
            .collect();
        Ok(Response::new(pb::ListAuditKeysResponse { keys }))
    }

    type WatchAuditStream = EntryStream;

    async fn watch_audit(
        &self,
        request: Request<pb::WatchAuditRequest>,
    ) -> GrpcResult<Self::WatchAuditStream> {
        let org = authorized(&self.state, &request, &request.get_ref().org)
            .await
            .map_err(Status::from)?;
        let mut after = request.get_ref().after_seq;
        let audit = self.state.stores.audit();
        let shutdown = self.state.lifecycle.draining.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        tokio::spawn(async move {
            loop {
                let page = match audit.entries(&org, after, DEFAULT_LIMIT).await {
                    Ok(page) => page,
                    Err(e) => {
                        let _ = tx.send(Err(Status::from(Error::from(e)))).await;
                        return;
                    }
                };
                for entry in &page {
                    after = entry.seq;
                    let item = entry_proto(entry).map_err(Status::from);
                    if tx.send(item).await.is_err() {
                        return;
                    }
                }
                if page.is_empty() {
                    tokio::select! {
                        _ = shutdown.cancelled() => return,
                        _ = tx.closed() => return,
                        _ = tokio::time::sleep(WATCH_POLL) => {}
                    }
                }
            }
        });
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }
}
