//! Repository webhooks and the caller's notification preferences. Results
//! are the wire records.

use rill_proto::rill::v1 as pb;
use rill_types::OperationId;

use crate::client::Client;
use crate::error::Result;
use crate::names;

/// Notification operations on one repository.
#[derive(Clone, Debug)]
pub struct NotificationClient {
    client: Client,
    repo: String,
}

impl NotificationClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        NotificationClient { client, repo }
    }

    fn svc(&self) -> crate::client::NotificationSvc {
        self.client.notification_svc()
    }

    /// The repository's webhooks.
    pub async fn webhooks(&self) -> Result<Vec<pb::Webhook>> {
        let request = pb::ListWebhooksRequest {
            parent: self.repo.clone(),
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_webhooks(request).await }
        })
        .await?
        .into_owned()
        .webhooks)
    }

    /// Add a webhook for `events` (every event when empty), signed with
    /// the org secret `secret` when given.
    pub async fn create_webhook(
        &self,
        url: &str,
        secret: Option<&str>,
        events: &[String],
    ) -> Result<pb::Webhook> {
        let request = pb::CreateWebhookRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: self.repo.clone(),
            webhook: pb::Webhook {
                url: url.to_string(),
                secret: secret.unwrap_or_default().to_string(),
                events: events.to_vec(),
                ..Default::default()
            }
            .into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_webhook(request).await }
        })
        .await?
        .into_owned();
        Ok(rill_proto::convert::required(
            "create_webhook.webhook",
            resp.webhook,
        )?)
    }

    /// Remove the webhook `id`.
    pub async fn delete_webhook(&self, id: &str) -> Result<()> {
        let request = pb::DeleteWebhookRequest {
            request_id: names::request_id(&OperationId::new()),
            name: format!("{}/webhooks/{id}", self.repo),
        };
        crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.delete_webhook(request).await }
        })
        .await?;
        Ok(())
    }

    /// The caller's preferences.
    pub async fn prefs(&self) -> Result<pb::NotificationPrefs> {
        let request = pb::GetNotificationPrefsRequest {
            parent: self.repo.clone(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.get_notification_prefs(request).await }
        })
        .await?
        .into_owned();
        Ok(resp.prefs.into_option().unwrap_or_default())
    }

    /// Replace the caller's preferences.
    pub async fn set_prefs(&self, prefs: pb::NotificationPrefs) -> Result<pb::NotificationPrefs> {
        let request = pb::UpdateNotificationPrefsRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: self.repo.clone(),
            prefs: prefs.into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.update_notification_prefs(request).await }
        })
        .await?
        .into_owned();
        Ok(resp.prefs.into_option().unwrap_or_default())
    }

    /// Send a test message to each of the caller's channels; how many.
    pub async fn send_test(&self) -> Result<u32> {
        let request = pb::SendTestNotificationRequest {
            parent: self.repo.clone(),
        };
        Ok(self
            .svc()
            .send_test_notification(request)
            .await?
            .into_owned()
            .queued)
    }
}
