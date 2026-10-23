//! Review mechanics: interdiffs between versions, applying a suggested
//! edit, and reopening an abandoned change.

use rill_core::{store_content, ChangeBuilder};
use rill_proto::rill::v1 as pb;
use rill_runtime::error::{Error, Result};
use rill_types::{ChangeVersionHash, FileChange, FileKind, RepoPathBuf};

use super::reviews::review_proto;
use super::ReviewSvc;
use crate::services::changes::commit_version;
use crate::services::common::{self, open_repo};
use crate::services::{names, Call};

impl ReviewSvc {
    pub(super) async fn get_interdiff_impl(
        &self,
        request: Call<pb::GetInterdiffRequest>,
    ) -> Result<pb::GetInterdiffResponse> {
        let req = request.get_ref();
        let (opened, id) = self.change(&request, &req.change).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let review = opened.repo.review();
        let state = review.get(&id).await?;
        let pick = |name: &str, back: usize| -> Result<ChangeVersionHash> {
            if name.is_empty() {
                let n = state.versions.len();
                return n
                    .checked_sub(1 + back)
                    .map(|i| state.versions[i])
                    .ok_or_else(|| {
                        Error::precondition(format!("{id} has fewer than {} versions", back + 1))
                    });
            }
            let v = names::version_in(&opened.repo, name)?;
            if !state.versions.contains(&v) {
                return Err(Error::invalid(format!("{v} is not a version of {id}")));
            }
            Ok(v)
        };
        let to = pick(&req.to, 0)?;
        let from = if req.from.is_empty() {
            let at = state
                .versions
                .iter()
                .position(|v| *v == to)
                .unwrap_or_default();
            at.checked_sub(1)
                .map(|i| state.versions[i])
                .ok_or_else(|| Error::precondition(format!("{to} is {id}'s first version")))?
        } else {
            pick(&req.from, 0)?
        };
        let diff = review.interdiff(&from, &to).await?;
        let filter = opened.permit().read_filter().await?;
        let files = diff
            .files
            .into_iter()
            .filter(|f| filter.allows(&f.path))
            .map(|f| pb::InterdiffFile {
                unified: String::from_utf8_lossy(&rill_core::to_unified(f.path.as_str(), &f.hunks))
                    .into_owned(),
                added: f.old.is_none() && f.new.is_some(),
                deleted: f.old.is_some() && f.new.is_none(),
                path: f.path.into_string(),
            })
            .collect();
        Ok(pb::GetInterdiffResponse {
            from: names::version(&opened.repo, &from),
            to: names::version(&opened.repo, &to),
            files,
            rebased: diff
                .rebased
                .into_iter()
                .filter(|p| filter.allows(p))
                .map(|p| p.into_string())
                .collect(),
        })
    }

    pub(super) async fn apply_suggestion_impl(
        &self,
        request: Call<pb::ApplySuggestionRequest>,
    ) -> Result<pb::ApplySuggestionResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.thread).await?;
        opened.permit().write_any().await?;
        let (change, thread) = names::thread_in(&opened.repo, &req.thread)?;
        let review = opened.repo.review();
        let state = review.get(&change).await?;
        opened.permit().owns_or_admin(&state.owner).await?;
        let edit = review.suggested_edit(&change, &thread).await?;
        opened.permit().write(&edit.path).await?;
        let ctx = &opened.repo;
        let cur = ctx.derive.change(&edit.version).await?;
        let root = common::root_of(ctx, &edit.version).await?;
        let kind = rill_core::diff::file_state(&ctx.trees, Some(root), &edit.path)
            .await?
            .map_or(FileKind::Regular, |(_, k)| k);
        let content = store_content(ctx.repo.objects().as_ref(), edit.content).await?;
        let mut builder = ChangeBuilder::new(opened.principal().clone())
            .message(cur.message.clone())
            .change_id(cur.change_id)
            .predecessor(edit.version);
        for parent in &cur.parents {
            builder = builder.parent(*parent);
        }
        if let Some(intent) = cur.intent {
            builder = builder.intent(intent);
        }
        for (path, sub) in &cur.subtree_changes {
            builder = builder.subtree(path.as_str(), sub.clone())?;
        }
        for (path, fc) in &cur.file_changes {
            if *path != edit.path {
                builder = builder.change(path.as_str(), fc.clone())?;
            }
        }
        builder = builder.change(edit.path.as_str(), FileChange::set(content, kind))?;
        let cv = builder.build(ctx.repo.objects().as_ref()).await?;
        let (hash, _) = commit_version(&opened, cv).await?;
        // Storing the version added it to the review; the suggestion is done.
        if let Err(e) = review.resolve(&change, &thread, opened.principal()).await {
            tracing::debug!(error = %e, "resolving an applied suggestion");
        }
        Ok(pb::ApplySuggestionResponse {
            info: common::change_info(ctx, &hash).await?.into(),
        })
    }

    pub(super) async fn reopen_change_impl(
        &self,
        request: Call<pb::ReopenChangeRequest>,
    ) -> Result<pb::ReopenChangeResponse> {
        let (opened, id) = self.change(&request, &request.get_ref().change).await?;
        opened.permit().write_any().await?;
        let review = opened.repo.review();
        let current = review.get(&id).await?;
        opened.permit().owns_or_admin(&current.owner).await?;
        let state = review.reopen(&id).await?;
        Ok(pb::ReopenChangeResponse {
            review: review_proto(&opened.repo, state).into(),
        })
    }
}
