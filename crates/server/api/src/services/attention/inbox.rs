//! The caller's inbox.

use rill_policy::owners::Approver;
use rill_policy::{Relation, Verb};
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_review::attention::{InboxItem, InboxKind, Reader};
use rill_runtime::error::Result;
use rill_types::RepoPathBuf;

use super::AttentionSvc;
use crate::services::common::{limit, open_repo, Opened};
use crate::services::{names, Call};

/// Who the caller is, for matching inbox items.
pub(super) async fn reader(opened: &Opened) -> Reader {
    Reader {
        approver: Approver {
            principal: opened.principal().clone(),
            teams: opened.ctx.teams.clone(),
        },
        repo_owner: opened
            .permit()
            .check(Verb::Review, Relation::Owner, &RepoPathBuf::root())
            .await
            .is_ok(),
    }
}

fn item_proto(opened: &Opened, i: InboxItem) -> pb::InboxItem {
    let ctx = &opened.repo;
    pb::InboxItem {
        kind: match i.kind {
            InboxKind::Escalation => pb::InboxKind::Escalation,
            InboxKind::Audit => pb::InboxKind::Audit,
            InboxKind::OwnerReview => pb::InboxKind::OwnerReview,
            InboxKind::IntentApproval => pb::InboxKind::IntentApproval,
            InboxKind::Thread => pb::InboxKind::Thread,
        }
        .into(),
        change: i.change.map(|c| names::change(ctx, &c)).unwrap_or_default(),
        intent: i
            .intent
            .map(|c| names::intent_name(ctx, &c))
            .unwrap_or_default(),
        landing: i
            .landing
            .map(|s| names::trunk_position(ctx, s))
            .unwrap_or_default(),
        title: i.title,
        detail: i.detail,
        since: timestamp_to_proto(i.since),
        urgency: i.urgency,
    }
}

impl AttentionSvc {
    pub(super) async fn list_inbox_impl(
        &self,
        request: Call<pb::ListInboxRequest>,
    ) -> Result<pb::ListInboxResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let reader = reader(&opened).await;
        let items = opened
            .repo
            .review()
            .inbox(&reader, limit(req.limit, 50, 500))
            .await?;
        Ok(pb::ListInboxResponse {
            items: items.into_iter().map(|i| item_proto(&opened, i)).collect(),
        })
    }
}
