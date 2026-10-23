//! The `NotificationService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::NotificationService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `NotificationService`.
#[derive(Clone)]
pub struct NotificationSvc {
    pub(super) state: Arc<AppState>,
}

impl NotificationSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        NotificationSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl NotificationService for NotificationSvc {
    async fn list_webhooks(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListWebhooksRequest>,
    ) -> GrpcResult<pb::ListWebhooksResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_webhooks_impl(request).await?))
    }

    async fn create_webhook(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateWebhookRequest>,
    ) -> GrpcResult<pb::CreateWebhookResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_webhook_impl(r).await?))
            })
            .await?,
        )
    }

    async fn delete_webhook(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteWebhookRequest>,
    ) -> GrpcResult<pb::DeleteWebhookResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.delete_webhook_impl(r).await?))
            })
            .await?,
        )
    }

    async fn get_notification_prefs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetNotificationPrefsRequest>,
    ) -> GrpcResult<pb::GetNotificationPrefsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(
            self.get_notification_prefs_impl(request).await?,
        ))
    }

    async fn update_notification_prefs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateNotificationPrefsRequest>,
    ) -> GrpcResult<pb::UpdateNotificationPrefsResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.update_notification_prefs_impl(r).await?))
            })
            .await?,
        )
    }

    async fn send_test_notification(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::SendTestNotificationRequest>,
    ) -> GrpcResult<pb::SendTestNotificationResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(
            self.send_test_notification_impl(request).await?,
        ))
    }
}
