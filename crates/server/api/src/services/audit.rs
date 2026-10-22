//! `rill.v1.AuditService`: reading, sealing and following an org's audit
//! chain.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::Stream;
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::AuditService;
use rill_types::audit::{AuditEntry, AuditOutcome, AuditRoot, INSTANCE_AUDIT};

use crate::auth::{caller, require_instance_admin, require_org_admin};
use crate::listing::{Page, Position};
use crate::services::Call;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

type GrpcResult<T> = std::result::Result<Response<T>, ConnectError>;

const DEFAULT_LIMIT: usize = 500;
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

/// The org a request's parent names, once the caller may read its audit
/// log.
async fn authorized<T>(state: &AppState, request: &Call<T>, parent: &str) -> Result<String> {
    let ctx = caller(request)?;
    let org = super::names::org(parent)?;
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
        actor: pb::Principal::from(r.actor.clone()).into(),
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
    Pin<Box<dyn Stream<Item = std::result::Result<pb::WatchAuditResponse, ConnectError>> + Send>>;

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl AuditService for AuditSvc {
    async fn list_audit_entries(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListAuditEntriesRequest>,
    ) -> GrpcResult<pb::ListAuditEntriesResponse> {
        let request = Call::new(ctx, request);
        let result: Result<_> = async {
            let req = request.get_ref();
            let org = authorized(&self.state, &request, &req.parent).await?;
            let after = req.after_seq.to_string();
            let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &after])?;
            let after = match &page.start {
                Position::Start => req.after_seq,
                Position::After(key) => u64::from_be_bytes(
                    key.as_slice()
                        .try_into()
                        .map_err(|_| Error::InvalidPageToken("not a token for this list".into()))?,
                ),
                Position::Offset(_) => {
                    return Err(Error::InvalidPageToken("not a token for this list".into()))
                }
            };
            let audit = self.state.stores.audit();
            let found = audit.entries(&org, after, page.size).await?;
            let next_page_token = match (found.len() == page.size, found.last()) {
                (true, Some(last)) => page.token(Position::After(last.seq.to_be_bytes().to_vec())),
                _ => String::new(),
            };
            let entries = found.iter().map(entry_proto).collect::<Result<_>>()?;
            Ok(pb::ListAuditEntriesResponse {
                entries,
                head_seq: audit.head(&org).await?.seq,
                next_page_token,
            })
        }
        .await;
        result.map(Response::new).map_err(ConnectError::from)
    }

    async fn list_audit_roots(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListAuditRootsRequest>,
    ) -> GrpcResult<pb::ListAuditRootsResponse> {
        let request = Call::new(ctx, request);
        let result: Result<_> = async {
            let req = request.get_ref();
            let org = authorized(&self.state, &request, &req.parent).await?;
            let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
            let (roots, next_page_token) =
                page.of(self.state.stores.audit().roots(&org).await?)?;
            Ok(pb::ListAuditRootsResponse {
                roots: roots.iter().map(root_proto).collect::<Result<_>>()?,
                next_page_token,
            })
        }
        .await;
        result.map(Response::new).map_err(ConnectError::from)
    }

    async fn seal_audit(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::SealAuditRequest>,
    ) -> GrpcResult<pb::SealAuditResponse> {
        let request = Call::new(ctx, request);
        let result: Result<_> = async {
            let org = authorized(&self.state, &request, &request.get_ref().parent).await?;
            rill_runtime::audit::worker::catch_up(&self.state).await?;
            let root = rill_runtime::audit::seal::seal(&self.state, &org).await?;
            Ok(pb::SealAuditResponse {
                root: root.as_ref().map(root_proto).transpose()?.into(),
            })
        }
        .await;
        result.map(Response::new).map_err(ConnectError::from)
    }

    async fn list_audit_keys(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListAuditKeysRequest>,
    ) -> GrpcResult<pb::ListAuditKeysResponse> {
        let request = Call::new(ctx, request);
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

    async fn watch_audit(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchAuditRequest>,
    ) -> GrpcResult<EntryStream> {
        let request = Call::new(ctx, request);
        let org = authorized(&self.state, &request, &request.get_ref().parent)
            .await
            .map_err(ConnectError::from)?;
        let mut after = request.get_ref().after_seq;
        let audit = self.state.stores.audit();
        let shutdown = self.state.lifecycle.draining.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        tokio::spawn(async move {
            loop {
                let page = match audit.entries(&org, after, DEFAULT_LIMIT).await {
                    Ok(page) => page,
                    Err(e) => {
                        let _ = tx.send(Err(ConnectError::from(Error::from(e)))).await;
                        return;
                    }
                };
                for entry in &page {
                    after = entry.seq;
                    let item = entry_proto(entry)
                        .map(|entry| pb::WatchAuditResponse {
                            entry: entry.into(),
                        })
                        .map_err(ConnectError::from);
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
