//! Intents: creation, lookup, update, listing, linking and similarity
//! search.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, Intent, IntentId, OperationId};

use crate::client::Client;
use crate::error::Result;
use crate::names;
use crate::types::{IntentFilter, IntentUpdate, NewIntent, Page, ScoredIntent};

/// Intent operations on one repository.
#[derive(Clone, Debug)]
pub struct IntentClient {
    client: Client,
    repo: String,
}

impl IntentClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        IntentClient { client, repo }
    }

    pub(crate) fn svc(&self) -> crate::client::IntentSvc {
        self.client.intent_svc()
    }

    pub(crate) fn repo_name(&self) -> &str {
        &self.repo
    }

    pub(crate) fn name(&self, id: &IntentId) -> String {
        names::intent(&self.repo, id)
    }

    /// Create an intent.
    pub async fn create(&self, intent: NewIntent) -> Result<Intent> {
        self.create_with_op(OperationId::new(), intent).await
    }

    /// [`IntentClient::create`] with a caller-chosen idempotency key.
    pub async fn create_with_op(&self, op: OperationId, intent: NewIntent) -> Result<Intent> {
        let request = pb::CreateIntentRequest {
            request_id: names::request_id(&op),
            parent: self.repo.clone(),
            title: intent.title,
            body: intent.body,
            kind: pb::IntentKind::from(intent.kind).into(),
            parent_intent: intent.parent.map(|p| self.name(&p)).unwrap_or_default(),
            risk_override: intent.risk_override.map(|r| pb::RiskClass::from(r).into()),
            scope: intent.scope.as_ref().map(pb::IntentScope::from).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_intent(request).await }
        })
        .await?
        .into_owned();
        Ok(required("create_intent.intent", resp.intent)?.try_into()?)
    }

    /// An intent by id.
    pub async fn get(&self, id: IntentId) -> Result<Intent> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetIntentRequest {
                name: self.name(&id),
            };
            async move { svc.get_intent(request).await }
        })
        .await?
        .into_owned();
        Ok(required("get_intent.intent", resp.intent)?.try_into()?)
    }

    /// Partially update an intent.
    pub async fn update(&self, id: IntentId, update: IntentUpdate) -> Result<Intent> {
        self.update_with_op(OperationId::new(), id, update).await
    }

    /// [`IntentClient::update`] with a caller-chosen idempotency key.
    pub async fn update_with_op(
        &self,
        op: OperationId,
        id: IntentId,
        update: IntentUpdate,
    ) -> Result<Intent> {
        let (intent, paths) = update.to_proto(self.name(&id));
        let request = pb::UpdateIntentRequest {
            request_id: names::request_id(&op),
            intent: intent.into(),
            update_mask: rill_proto::FieldMask {
                paths,
                ..Default::default()
            }
            .into(),
            etag: String::new(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.update_intent(request).await }
        })
        .await?
        .into_owned();
        Ok(required("update_intent.intent", resp.intent)?.try_into()?)
    }

    /// Intents matching a filter.
    pub async fn list(&self, filter: IntentFilter) -> Result<Page<Intent>> {
        let request = pb::ListIntentsRequest {
            parent: self.repo.clone(),
            page_size: filter.limit as i32,
            page_token: filter.page_token.clone().unwrap_or_default(),
            filter: filter.to_filter(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_intents(request).await }
        })
        .await?
        .into_owned();
        let items = resp
            .intents
            .into_iter()
            .map(|i| Intent::try_from(i).map_err(Into::into))
            .collect::<Result<_>>()?;
        Ok(Page::new(
            items,
            Some(resp.next_page_token).filter(|t| !t.is_empty()),
        ))
    }

    /// Link a change to an intent.
    pub async fn link_change(&self, intent: IntentId, change_id: ChangeId) -> Result<Intent> {
        self.link_change_with_op(OperationId::new(), intent, change_id)
            .await
    }

    /// [`IntentClient::link_change`] with a caller-chosen idempotency key.
    pub async fn link_change_with_op(
        &self,
        op: OperationId,
        intent: IntentId,
        change_id: ChangeId,
    ) -> Result<Intent> {
        let request = pb::LinkChangeRequest {
            request_id: names::request_id(&op),
            name: self.name(&intent),
            change: names::change(&self.repo, &change_id),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.link_change(request).await }
        })
        .await?
        .into_owned();
        Ok(required("link_change.intent", resp.intent)?.try_into()?)
    }

    /// Intents similar to a title and body, best first.
    pub async fn find_similar(
        &self,
        title: &str,
        body: &str,
        limit: u32,
    ) -> Result<Vec<ScoredIntent>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::FindSimilarIntentsRequest {
                parent: self.repo.clone(),
                title: title.to_string(),
                body: body.to_string(),
                max_results: limit as i32,
            };
            async move { svc.find_similar_intents(request).await }
        })
        .await?
        .into_owned();
        resp.results
            .into_iter()
            .map(ScoredIntent::try_from)
            .collect()
    }
}
