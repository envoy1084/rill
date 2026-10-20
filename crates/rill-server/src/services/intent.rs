//! `rill.v1.IntentService`: the "why" behind changes.

use std::collections::BTreeSet;
use std::sync::Arc;

use rill_proto::convert::{
    id_from_proto, intent_kind_from_proto, intent_status_from_proto, opt_id_from_proto,
    risk_from_proto,
};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::intent_service_server::IntentService;
use rill_types::{ChangeId, Intent, IntentId, IntentKind, IntentStatus, RepoPathBuf, Timestamp};
use tonic::{Request, Response, Status};

use super::common::{self, open, GrpcResult, Opened};
use crate::error::{Error, Result};
use crate::state::AppState;

/// The intent service.
#[derive(Clone)]
pub struct IntentSvc {
    state: Arc<AppState>,
}

impl IntentSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        IntentSvc { state }
    }
}

async fn load(opened: &Opened, id: &[u8]) -> Result<Intent> {
    let id: IntentId = id_from_proto("id", id)?;
    opened
        .repo
        .repo
        .get_intent(&id)
        .await?
        .ok_or_else(|| Error::not_found(format!("intent {id}")))
}

#[tonic::async_trait]
impl IntentService for IntentSvc {
    async fn create_intent(
        &self,
        request: Request<pb::CreateIntentRequest>,
    ) -> GrpcResult<pb::CreateIntentResponse> {
        let result: crate::error::Result<Response<pb::CreateIntentResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().write(&RepoPathBuf::root()).await?;
            let req = request.into_inner();
            if req.title.trim().is_empty() {
                return Err(Error::invalid("title is required"));
            }
            let kind = intent_kind_from_proto("kind", req.kind).unwrap_or(IntentKind::Feature);
            let mut intent = Intent::new(req.title, kind, opened.principal().clone());
            intent.body = req.body;
            intent.parent = opt_id_from_proto("parent", req.parent.as_ref())?;
            intent.risk_override = req
                .risk_override
                .map(|r| risk_from_proto("risk_override", r))
                .transpose()?;
            if let Some(parent) = intent.parent {
                if opened.repo.repo.get_intent(&parent).await?.is_none() {
                    return Err(Error::not_found(format!("parent intent {parent}")));
                }
            }
            opened.repo.repo.put_intent(&intent).await?;
            Ok(Response::new(pb::CreateIntentResponse {
                intent: Some(pb::Intent::from(intent)),
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn get_intent(
        &self,
        request: Request<pb::GetIntentRequest>,
    ) -> GrpcResult<pb::GetIntentResponse> {
        let result: crate::error::Result<Response<pb::GetIntentResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let intent = load(&opened, &request.get_ref().id).await?;
            Ok(Response::new(pb::GetIntentResponse {
                intent: Some(pb::Intent::from(intent)),
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn update_intent(
        &self,
        request: Request<pb::UpdateIntentRequest>,
    ) -> GrpcResult<pb::UpdateIntentResponse> {
        let result: crate::error::Result<Response<pb::UpdateIntentResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().write(&RepoPathBuf::root()).await?;
            let req = request.into_inner();
            let mut intent = load(&opened, &req.id).await?;
            opened.permit().owns_or_admin(&intent.owner).await?;
            if let Some(title) = req.title {
                intent.title = title;
            }
            if let Some(body) = req.body {
                intent.body = body;
            }
            if let Some(status) = req.status {
                intent.status = intent_status_from_proto("status", status)?;
            }
            if req.clear_parent {
                intent.parent = None;
            } else if let Some(parent) =
                opt_id_from_proto::<IntentId>("parent", req.parent.as_ref())?
            {
                if parent == intent.id {
                    return Err(Error::invalid("an intent cannot be its own parent"));
                }
                intent.parent = Some(parent);
            }
            if req.clear_risk_override {
                intent.risk_override = None;
            } else if let Some(risk) = req.risk_override {
                intent.risk_override = Some(risk_from_proto("risk_override", risk)?);
            }
            intent.updated_at = Timestamp::now();
            opened.repo.repo.put_intent(&intent).await?;
            Ok(Response::new(pb::UpdateIntentResponse {
                intent: Some(pb::Intent::from(intent)),
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn list_intents(
        &self,
        request: Request<pb::ListIntentsRequest>,
    ) -> GrpcResult<pb::ListIntentsResponse> {
        let result: crate::error::Result<Response<pb::ListIntentsResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let req = request.get_ref();
            let statuses: Vec<IntentStatus> = req
                .status
                .iter()
                .map(|s| intent_status_from_proto("status", *s))
                .collect::<std::result::Result<_, _>>()?;
            let kinds: Vec<IntentKind> = req
                .kind
                .iter()
                .map(|k| intent_kind_from_proto("kind", *k))
                .collect::<std::result::Result<_, _>>()?;
            let owner: Option<rill_types::PrincipalId> =
                opt_id_from_proto("owner", req.owner.as_ref())?;
            let parent: Option<IntentId> = opt_id_from_proto("parent", req.parent.as_ref())?;
            let limit = common::limit(req.limit, 100, 10_000);
            let mut intents = opened.repo.repo.list_intents().await?;
            intents.retain(|i| {
                (statuses.is_empty() || statuses.contains(&i.status))
                    && (kinds.is_empty() || kinds.contains(&i.kind))
                    && owner.is_none_or(|o| i.owner.id == o)
                    && parent.is_none_or(|p| i.parent == Some(p))
            });
            intents.sort_by_key(|i| std::cmp::Reverse(i.updated_at));
            intents.truncate(limit);
            Ok(Response::new(pb::ListIntentsResponse {
                intents: intents.into_iter().map(pb::Intent::from).collect(),
                next_page_token: None,
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    /// Link a change to an intent on both sides: the intent's change list
    /// and, when the change is under review, the review's intent.
    async fn link_change(
        &self,
        request: Request<pb::LinkChangeRequest>,
    ) -> GrpcResult<pb::LinkChangeResponse> {
        let result: crate::error::Result<Response<pb::LinkChangeResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().write(&RepoPathBuf::root()).await?;
            let req = request.get_ref();
            let mut intent = load(&opened, &req.intent).await?;
            let change_id: ChangeId = id_from_proto("change_id", &req.change_id)?;
            if opened
                .repo
                .repo
                .change_versions(&change_id)
                .await?
                .is_empty()
            {
                return Err(Error::not_found(format!("change {change_id}")));
            }
            if !intent.changes.contains(&change_id) {
                intent.changes.push(change_id);
                intent.updated_at = Timestamp::now();
                if intent.status == IntentStatus::Open {
                    intent.status = IntentStatus::InProgress;
                }
                opened.repo.repo.put_intent(&intent).await?;
            }
            if let Some(prev) = opened.repo.repo.get_review(&change_id).await? {
                if prev.intent != Some(intent.id) {
                    let mut next = prev.clone();
                    next.intent = Some(intent.id);
                    next.updated_at = Timestamp::now();
                    opened.repo.repo.put_review(&next, Some(&prev)).await?;
                }
            }
            Ok(Response::new(pb::LinkChangeResponse {
                intent: Some(pb::Intent::from(intent)),
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    /// Token-overlap (Jaccard) similarity over title and body. Good
    /// enough to catch duplicates with shared vocabulary; embeddings are
    /// the upgrade path when a vector index is available.
    async fn find_similar_intents(
        &self,
        request: Request<pb::FindSimilarIntentsRequest>,
    ) -> GrpcResult<pb::FindSimilarIntentsResponse> {
        let result: crate::error::Result<Response<pb::FindSimilarIntentsResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let req = request.get_ref();
            let query = tokens(&format!("{} {}", req.title, req.body));
            if query.is_empty() {
                return Ok(Response::new(pb::FindSimilarIntentsResponse {
                    results: Vec::new(),
                }));
            }
            let limit = common::limit(req.limit, 10, 100);
            let mut scored: Vec<(f32, Intent)> = opened
                .repo
                .repo
                .list_intents()
                .await?
                .into_iter()
                .filter_map(|intent| {
                    let score = jaccard(
                        &query,
                        &tokens(&format!("{} {}", intent.title, intent.body)),
                    );
                    (score > 0.0).then_some((score, intent))
                })
                .collect();
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(limit);
            Ok(Response::new(pb::FindSimilarIntentsResponse {
                results: scored
                    .into_iter()
                    .map(|(score, intent)| pb::ScoredIntent {
                        intent: Some(pb::Intent::from(intent)),
                        score,
                    })
                    .collect(),
            }))
        }
        .await;
        result.map_err(Status::from)
    }
}

/// Lower-cased alphanumeric words of at least three characters.
fn tokens(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_lowercase)
        .collect()
}

fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f32 {
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f32 / union as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_scores_overlap() {
        let a = tokens("Fix login timeout on slow networks");
        let b = tokens("Login timeout fix for slow network");
        let c = tokens("Add dark mode");
        assert!(jaccard(&a, &b) > jaccard(&a, &c));
        assert_eq!(jaccard(&a, &c), 0.0);
        assert_eq!(jaccard(&a, &a), 1.0);
    }
}
