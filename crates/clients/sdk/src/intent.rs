//! Intents: creation, lookup, update, listing, linking and similarity
//! search.

use rill_proto::convert::{id_to_proto, required};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, Intent, IntentId, OperationId};

use crate::client::Client;
use crate::error::Result;
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

    fn svc(&self) -> crate::client::IntentSvc {
        self.client.intent_svc()
    }

    /// Create an intent.
    pub async fn create(&self, intent: NewIntent) -> Result<Intent> {
        self.create_with_op(OperationId::new(), intent).await
    }

    /// [`IntentClient::create`] with a caller-chosen idempotency key.
    pub async fn create_with_op(&self, op: OperationId, intent: NewIntent) -> Result<Intent> {
        let request = pb::CreateIntentRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            title: intent.title,
            body: intent.body,
            kind: pb::IntentKind::from(intent.kind).into(),
            parent: intent.parent.as_ref().map(id_to_proto),
            risk_override: intent.risk_override.map(|r| pb::RiskClass::from(r).into()),
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
                repo: self.repo.clone(),
                id: id_to_proto(&id),
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
        let request = pb::UpdateIntentRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            id: id_to_proto(&id),
            title: update.title,
            body: update.body,
            status: update.status.map(|s| pb::IntentStatus::from(s).into()),
            parent: update.parent.value().map(id_to_proto),
            clear_parent: update.parent.clears(),
            risk_override: update
                .risk_override
                .value()
                .map(|r| pb::RiskClass::from(*r).into()),
            clear_risk_override: update.risk_override.clears(),
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
            repo: self.repo.clone(),
            status: filter
                .status
                .into_iter()
                .map(|s| pb::IntentStatus::from(s).into())
                .collect(),
            kind: filter
                .kind
                .into_iter()
                .map(|k| pb::IntentKind::from(k).into())
                .collect(),
            owner: filter.owner.as_ref().map(id_to_proto),
            parent: filter.parent.as_ref().map(id_to_proto),
            limit: filter.limit,
            page_token: filter.page_token,
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
        Ok(Page::new(items, resp.next_page_token))
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
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            intent: id_to_proto(&intent),
            change_id: id_to_proto(&change_id),
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
                repo: self.repo.clone(),
                title: title.to_string(),
                body: body.to_string(),
                limit,
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
