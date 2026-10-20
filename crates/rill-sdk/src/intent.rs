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
        let resp = self
            .svc()
            .create_intent(pb::CreateIntentRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                title: intent.title,
                body: intent.body,
                kind: pb::IntentKind::from(intent.kind) as i32,
                parent: intent.parent.as_ref().map(id_to_proto),
                risk_override: intent.risk_override.map(|r| pb::RiskClass::from(r) as i32),
            })
            .await?
            .into_inner();
        Ok(required("create_intent.intent", resp.intent)?.try_into()?)
    }

    /// An intent by id.
    pub async fn get(&self, id: IntentId) -> Result<Intent> {
        let resp = self
            .svc()
            .get_intent(pb::GetIntentRequest {
                repo: self.repo.clone(),
                id: id_to_proto(&id),
            })
            .await?
            .into_inner();
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
        let resp = self
            .svc()
            .update_intent(pb::UpdateIntentRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                id: id_to_proto(&id),
                title: update.title,
                body: update.body,
                status: update.status.map(|s| pb::IntentStatus::from(s) as i32),
                parent: update.parent.value().map(id_to_proto),
                clear_parent: update.parent.clears(),
                risk_override: update
                    .risk_override
                    .value()
                    .map(|r| pb::RiskClass::from(*r) as i32),
                clear_risk_override: update.risk_override.clears(),
            })
            .await?
            .into_inner();
        Ok(required("update_intent.intent", resp.intent)?.try_into()?)
    }

    /// Intents matching a filter.
    pub async fn list(&self, filter: IntentFilter) -> Result<Page<Intent>> {
        let resp = self
            .svc()
            .list_intents(pb::ListIntentsRequest {
                repo: self.repo.clone(),
                status: filter
                    .status
                    .into_iter()
                    .map(|s| pb::IntentStatus::from(s) as i32)
                    .collect(),
                kind: filter
                    .kind
                    .into_iter()
                    .map(|k| pb::IntentKind::from(k) as i32)
                    .collect(),
                owner: filter.owner.as_ref().map(id_to_proto),
                parent: filter.parent.as_ref().map(id_to_proto),
                limit: filter.limit,
                page_token: filter.page_token,
            })
            .await?
            .into_inner();
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
        let resp = self
            .svc()
            .link_change(pb::LinkChangeRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                intent: id_to_proto(&intent),
                change_id: id_to_proto(&change_id),
            })
            .await?
            .into_inner();
        Ok(required("link_change.intent", resp.intent)?.try_into()?)
    }

    /// Intents similar to a title and body, best first.
    pub async fn find_similar(
        &self,
        title: &str,
        body: &str,
        limit: u32,
    ) -> Result<Vec<ScoredIntent>> {
        let resp = self
            .svc()
            .find_similar_intents(pb::FindSimilarIntentsRequest {
                repo: self.repo.clone(),
                title: title.to_string(),
                body: body.to_string(),
                limit,
            })
            .await?
            .into_inner();
        resp.results
            .into_iter()
            .map(ScoredIntent::try_from)
            .collect()
    }
}
