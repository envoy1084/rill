//! Creating, reading, updating, listing and linking intents.

use rill_proto::convert::{
    intent_kind_from_proto, intent_status_from_proto, opt_id_from_proto, risk_from_proto,
};
use rill_proto::rill::v1 as pb;
use rill_types::{Intent, IntentId, IntentKind, IntentStatus, RepoPathBuf, Timestamp};

use super::IntentSvc;
use crate::listing::{FieldValue, Filter, Kind, Page, Record};
use crate::services::common::{open_repo, Opened};
use crate::services::{etag, names, Call};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

/// The fields `UpdateIntent` may change.
const MASKABLE: &[&str] = &["title", "body", "status", "parent", "risk_override"];

/// The wire form of an intent, with its name and etag.
pub(super) fn intent_proto(ctx: &RepoContext, intent: Intent) -> pb::Intent {
    let name = names::intent_name(ctx, &intent.id);
    let mut out = pb::Intent::from(intent);
    out.name = name;
    out.etag = etag::of(&out);
    out
}

async fn load(opened: &Opened, name: &str) -> Result<Intent> {
    let id = names::intent_in(&opened.repo, name)?;
    opened
        .repo
        .repo
        .get_intent(&id)
        .await?
        .ok_or_else(|| Error::not_found(format!("intent {id}")))
}

impl IntentSvc {
    pub(super) async fn create_intent_impl(
        &self,
        request: Call<pb::CreateIntentRequest>,
    ) -> Result<pb::CreateIntentResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().write(&RepoPathBuf::root()).await?;
        let req = request.into_inner();
        if req.title.trim().is_empty() {
            return Err(rill_proto::ConvertError::missing("title").into());
        }
        let kind = intent_kind_from_proto("kind", req.kind).unwrap_or(IntentKind::Feature);
        let mut intent = Intent::new(req.title, kind, opened.principal().clone());
        intent.body = req.body;
        if !req.parent_intent.is_empty() {
            let parent = load(&opened, &req.parent_intent).await?;
            intent.parent = Some(parent.id);
        }
        intent.risk_override = req
            .risk_override
            .map(|r| risk_from_proto("risk_override", r))
            .transpose()?;
        opened
            .repo
            .repo
            .put_intent(&intent, opened.principal())
            .await?;
        Ok(pb::CreateIntentResponse {
            intent: intent_proto(&opened.repo, intent).into(),
        })
    }

    pub(super) async fn get_intent_impl(
        &self,
        request: Call<pb::GetIntentRequest>,
    ) -> Result<pb::GetIntentResponse> {
        let name = &request.get_ref().name;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let intent = load(&opened, name).await?;
        Ok(pb::GetIntentResponse {
            intent: intent_proto(&opened.repo, intent).into(),
        })
    }

    pub(super) async fn update_intent_impl(
        &self,
        request: Call<pb::UpdateIntentRequest>,
    ) -> Result<pb::UpdateIntentResponse> {
        let req = request.get_ref();
        let given = req
            .intent
            .as_option()
            .ok_or(rill_proto::ConvertError::missing("intent"))?;
        let opened = open_repo(&self.state, &request, &given.name).await?;
        opened.permit().write(&RepoPathBuf::root()).await?;
        let req = request.into_inner();
        let given = req.intent.into_option().unwrap_or_default();
        let mask = req.update_mask.into_option().unwrap_or_default().paths;
        if mask.is_empty() {
            return Err(rill_proto::ConvertError::missing("update_mask").into());
        }
        if let Some(bad) = mask.iter().find(|p| !MASKABLE.contains(&p.as_str())) {
            return Err(Error::invalid(format!(
                "update_mask names {bad:?}; it may name {}",
                MASKABLE.join(", ")
            )));
        }
        let mut intent = load(&opened, &given.name).await?;
        etag::check(
            &intent_proto(&opened.repo, intent.clone()).etag,
            &req.etag,
            "the intent",
        )?;
        opened.permit().owns_or_admin(&intent.owner).await?;
        for path in &mask {
            match path.as_str() {
                "title" => intent.title = given.title.clone(),
                "body" => intent.body = given.body.clone(),
                "status" => intent.status = intent_status_from_proto("status", given.status)?,
                "parent" => {
                    let parent: Option<IntentId> =
                        opt_id_from_proto("parent", given.parent.as_ref())?;
                    if parent == Some(intent.id) {
                        return Err(Error::invalid("an intent cannot be its own parent"));
                    }
                    intent.parent = parent;
                }
                _ => {
                    intent.risk_override = given
                        .risk_override
                        .map(|r| risk_from_proto("risk_override", r))
                        .transpose()?
                }
            }
        }
        intent.updated_at = Timestamp::now();
        opened
            .repo
            .repo
            .put_intent(&intent, opened.principal())
            .await?;
        Ok(pb::UpdateIntentResponse {
            intent: intent_proto(&opened.repo, intent).into(),
        })
    }

    pub(super) async fn list_intents_impl(
        &self,
        request: Call<pb::ListIntentsRequest>,
    ) -> Result<pb::ListIntentsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let filter = Filter::parse(
            &req.filter,
            &[
                ("status", Kind::Str),
                ("kind", Kind::Str),
                ("owner", Kind::Str),
                ("parent", Kind::Str),
            ],
        )?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.filter])?;
        let mut intents = opened.repo.repo.list_intents().await?;
        intents.retain(|i| filter.matches(&Listed(i)));
        intents.sort_by_key(|i| (std::cmp::Reverse(i.updated_at), i.id));
        let (intents, next_page_token) = page.of(intents)?;
        Ok(pb::ListIntentsResponse {
            intents: intents
                .into_iter()
                .map(|i| intent_proto(&opened.repo, i))
                .collect(),
            next_page_token,
        })
    }

    /// Link a change to an intent on both sides: the intent's change list
    /// and, when the change is under review, the review's intent.
    pub(super) async fn link_change_impl(
        &self,
        request: Call<pb::LinkChangeRequest>,
    ) -> Result<pb::LinkChangeResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.name).await?;
        opened.permit().write(&RepoPathBuf::root()).await?;
        let mut intent = load(&opened, &req.name).await?;
        let change_id = names::change_in(&opened.repo, &req.change)?;
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
            opened
                .repo
                .repo
                .put_intent(&intent, opened.principal())
                .await?;
        }
        if let Some(prev) = opened.repo.repo.read_review(&change_id).await? {
            if prev.state.intent != Some(intent.id) {
                let mut next = prev.state.clone();
                next.intent = Some(intent.id);
                next.updated_at = Timestamp::now();
                opened
                    .repo
                    .repo
                    .write_review(&prev, &next, opened.principal())
                    .await?;
            }
        }
        Ok(pb::LinkChangeResponse {
            intent: intent_proto(&opened.repo, intent).into(),
        })
    }
}

/// An intent as a list filter sees it.
struct Listed<'a>(&'a Intent);

impl Record for Listed<'_> {
    fn field(&self, field: &str) -> FieldValue {
        match field {
            "status" => FieldValue::Str(
                match self.0.status {
                    IntentStatus::Open => "open",
                    IntentStatus::InProgress => "in_progress",
                    IntentStatus::Done => "done",
                    IntentStatus::Abandoned => "abandoned",
                }
                .into(),
            ),
            "kind" => FieldValue::Str(format!("{:?}", self.0.kind).to_lowercase()),
            "owner" => FieldValue::Str(self.0.owner.to_string()),
            "parent" => match self.0.parent {
                Some(p) => FieldValue::Str(p.to_string()),
                None => FieldValue::Missing,
            },
            _ => FieldValue::Missing,
        }
    }
}
