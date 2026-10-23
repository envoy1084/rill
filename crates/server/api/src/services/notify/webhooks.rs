//! A repository's webhooks, managed by its owners.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_runtime::error::{Error, Result};
use rill_runtime::notify::prefs::{self, check_url, Webhook, MAX_WEBHOOKS};
use rill_runtime::state::RepoContext;
use rill_types::Timestamp;

use super::NotificationSvc;
use crate::services::common::open_repo;
use crate::services::{names, Call};

fn webhook_name(ctx: &RepoContext, id: &str) -> String {
    rill_proto::names::WEBHOOK.format(&[&ctx.config.org, &ctx.config.name, id])
}

fn webhook_proto(ctx: &RepoContext, w: Webhook) -> pb::Webhook {
    pb::Webhook {
        name: webhook_name(ctx, &w.id),
        url: w.url,
        secret: w.secret.unwrap_or_default(),
        events: w.events,
        created_by: pb::Principal::from(w.created_by).into(),
        created_at: timestamp_to_proto(w.created_at),
    }
}

impl NotificationSvc {
    pub(super) async fn list_webhooks_impl(
        &self,
        request: Call<pb::ListWebhooksRequest>,
    ) -> Result<pb::ListWebhooksResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().admin().await?;
        let hooks = prefs::webhooks(&opened.repo.repo).await?;
        Ok(pb::ListWebhooksResponse {
            webhooks: hooks
                .into_iter()
                .map(|w| webhook_proto(&opened.repo, w))
                .collect(),
        })
    }

    pub(super) async fn create_webhook_impl(
        &self,
        request: Call<pb::CreateWebhookRequest>,
    ) -> Result<pb::CreateWebhookResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().admin().await?;
        let given = req.webhook.clone().into_option().unwrap_or_default();
        check_url(&given.url, self.state.config.notify.allow_insecure_urls)?;
        if !given.secret.is_empty() {
            rill_exec::remote::secrets::validate_secret_name(&given.secret)?;
        }
        let hook = Webhook {
            id: hex::encode(&rill_types::ids::random_id()[..8]),
            url: given.url,
            secret: (!given.secret.is_empty()).then_some(given.secret),
            events: given.events,
            created_by: opened.principal().clone(),
            created_at: Timestamp::now(),
        };
        let added = hook.clone();
        prefs::update_webhooks(&opened.repo.repo, |hooks| {
            if hooks.len() >= MAX_WEBHOOKS {
                return Err(Error::precondition(format!(
                    "a repository keeps at most {MAX_WEBHOOKS} webhooks"
                )));
            }
            hooks.push(added.clone());
            Ok(())
        })
        .await?;
        tracing::info!(repo = %opened.repo.config.name, url = %hook.url, "webhook added");
        Ok(pb::CreateWebhookResponse {
            webhook: webhook_proto(&opened.repo, hook).into(),
        })
    }

    pub(super) async fn delete_webhook_impl(
        &self,
        request: Call<pb::DeleteWebhookRequest>,
    ) -> Result<pb::DeleteWebhookResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.name).await?;
        opened.permit().admin().await?;
        let parts = names::parse(&rill_proto::names::WEBHOOK, &req.name)?;
        let id = parts[2].to_string();
        let removed = prefs::update_webhooks(&opened.repo.repo, |hooks| {
            let before = hooks.len();
            hooks.retain(|h| h.id != id);
            Ok(hooks.len() != before)
        })
        .await?;
        if !removed {
            return Err(Error::not_found(format!("webhook {id}")));
        }
        Ok(pb::DeleteWebhookResponse::default())
    }
}
