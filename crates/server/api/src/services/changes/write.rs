//! Writing history: creating, editing, rebasing, merging and resolving
//! change versions.

use std::collections::BTreeMap;

use bytes::Bytes;
use rill_core::{merge_changes, rebase, store_content, ChangeBuilder, MergeChangeOptions};
use rill_proto::rill::v1 as pb;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, ContentRef, FileChange, FileKind, IntentId,
    Provenance, RepoPathBuf, Signature, Timestamp, TreeHash,
};

use super::commit::commit_version;
use super::ChangeSvc;
use crate::services::common::{self, open_repo, path, provenance, Opened};
use crate::services::content::{check_content_exists, content_bytes};
use crate::services::{names, revision, Call};
use rill_runtime::error::{Error, Result};

/// The metadata of a version just stored.
async fn stored(opened: &Opened, hash: &ChangeVersionHash) -> Result<pb::ChangeInfo> {
    common::change_info(&opened.repo, hash).await
}

impl ChangeSvc {
    pub(super) async fn create_version_impl(
        &self,
        request: Call<pb::CreateVersionRequest>,
    ) -> Result<pb::CreateVersionResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().write_any().await?;
        let req = request.into_inner();
        let mut cv = ChangeVersion::try_from(
            req.version
                .into_option()
                .ok_or(rill_proto::ConvertError::missing("version"))?,
        )?;
        // Authorship and time are facts the server observes, never claims a
        // client makes; the hash is computed over the stamped values.
        cv.author = opened.principal().clone();
        cv.created_at = Timestamp::now();
        common::clear_server_fields(&mut cv.provenance);
        let signatures = req
            .signatures
            .into_iter()
            .map(Signature::try_from)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let (hash, _) = super::commit_version_with(
            &opened,
            cv,
            super::CommitExtras {
                signatures,
                ..Default::default()
            },
        )
        .await?;
        Ok(pb::CreateVersionResponse {
            version: stored(&opened, &hash).await?.into(),
        })
    }

    pub(super) async fn apply_edits_impl(
        &self,
        request: Call<pb::ApplyEditsRequest>,
    ) -> Result<pb::ApplyEditsResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        let req = request.into_inner();
        let ctx = &opened.repo;
        let objects = ctx.repo.objects();

        let amend = match req.change.as_str() {
            "" => None,
            name => Some(names::change_in(ctx, name)?),
        };
        let base = edit_base(&opened, req.base.as_option(), amend).await?;

        let mut builder = ChangeBuilder::new(opened.principal().clone())
            .message(req.message)
            .provenance(provenance(req.provenance.into_option())?);
        if let Some(id) = base.change_id {
            builder = builder.change_id(id);
        }
        for parent in &base.parents {
            builder = builder.parent(*parent);
        }
        if let Some(pred) = base.predecessor {
            builder = builder.predecessor(pred);
        }
        if let Some(intent) = names::intent(&req.intent)? {
            builder = builder.intent(intent);
        }

        if req.edits.len() > rill_types::limits::MAX_FILES_PER_CHANGE {
            return Err(Error::invalid(format!(
                "{} edits; the limit is {}",
                req.edits.len(),
                rill_types::limits::MAX_FILES_PER_CHANGE
            )));
        }
        for edit in req.edits {
            let path = path("edits.path", &edit.path)?;
            if path.is_root() {
                return Err(Error::invalid("an edit cannot target the repository root"));
            }
            opened.permit().write(&path).await?;
            let change = match edit.op {
                Some(pb::edit::Op::SetContent(set)) => set_content(objects.as_ref(), set).await?,
                Some(pb::edit::Op::Delete(_)) => FileChange::Delete,
                Some(pb::edit::Op::Patch(patch)) => {
                    let state = rill_core::diff::file_state(&ctx.trees, base.root, &path).await?;
                    let (old, kind) = match &state {
                        Some((content, kind)) => {
                            (content_bytes(objects.as_ref(), content).await?, *kind)
                        }
                        None => (Bytes::new(), FileKind::Regular),
                    };
                    let new = rill_core::apply_unified(&old, patch.as_bytes())
                        .map_err(|e| Error::invalid(format!("patch for {path:?}: {e}")))?;
                    FileChange::set(
                        store_content(objects.as_ref(), Bytes::from(new)).await?,
                        kind,
                    )
                }
                None => return Err(Error::invalid(format!("edit for {path:?} has no op"))),
            };
            builder = builder.change(path.as_str(), change)?;
        }
        let cv = builder.build(objects.as_ref()).await?;
        let (hash, cv) = commit_version(&opened, cv).await?;
        Ok(pb::ApplyEditsResponse {
            info: stored(&opened, &hash).await?.into(),
            version: pb::ChangeVersion::from(cv).into(),
        })
    }

    pub(super) async fn rebase_version_impl(
        &self,
        request: Call<pb::RebaseVersionRequest>,
    ) -> Result<pb::RebaseVersionResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        let req = request.get_ref();
        opened.permit().write_any().await?;
        let version = names::version_in(&opened.repo, &req.name)?;
        let new_parent = revision::resolve_version(&opened.repo, &req.onto).await?;
        let rebased = rebase(&opened.repo.derive, &version, &new_parent, Timestamp::now()).await?;
        let (hash, cv) = commit_version(&opened, rebased).await?;
        Ok(pb::RebaseVersionResponse {
            info: stored(&opened, &hash).await?.into(),
            version: pb::ChangeVersion::from(cv).into(),
        })
    }

    pub(super) async fn merge_versions_impl(
        &self,
        request: Call<pb::MergeVersionsRequest>,
    ) -> Result<pb::MergeVersionsResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        let req = request.into_inner();
        opened.permit().write_any().await?;
        let left = revision::resolve_version(&opened.repo, &req.left).await?;
        let right = revision::resolve_version(&opened.repo, &req.right).await?;
        let mut options = MergeChangeOptions::new(opened.principal().clone(), req.message);
        options.provenance = provenance(req.provenance.into_option())?;
        let mut merged = merge_changes(
            &opened.repo.derive,
            &opened.repo.graph,
            &left,
            &right,
            options,
        )
        .await?;
        merged.intent = names::intent(&req.intent)?;
        let (hash, cv) = commit_version(&opened, merged).await?;
        Ok(pb::MergeVersionsResponse {
            info: stored(&opened, &hash).await?.into(),
            version: pb::ChangeVersion::from(cv).into(),
        })
    }

    pub(super) async fn resolve_conflicts_impl(
        &self,
        request: Call<pb::ResolveConflictsRequest>,
    ) -> Result<pb::ResolveConflictsResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        opened.permit().write_any().await?;
        let req = request.into_inner();
        let ctx = &opened.repo;
        let version = names::version_in(ctx, &req.name)?;
        let mut cv = ctx.derive.change(&version).await?;
        if !cv.has_conflicts() {
            return Err(Error::precondition(format!(
                "version {version} has no conflicts to resolve"
            )));
        }
        for resolution in req.resolutions {
            let path = path("resolutions.path", &resolution.path)?;
            opened.permit().write(&path).await?;
            if !matches!(cv.file_changes.get(&path), Some(FileChange::Conflict(_))) {
                return Err(Error::invalid(format!(
                    "{path:?} is not conflicted in {version}"
                )));
            }
            let change = match resolution.resolution {
                Some(pb::resolution::Resolution::Set(set)) => {
                    set_content(ctx.repo.objects().as_ref(), set).await?
                }
                Some(pb::resolution::Resolution::Delete(_)) => FileChange::Delete,
                None => {
                    return Err(Error::invalid(format!(
                        "resolution for {path:?} has no value"
                    )))
                }
            };
            cv.file_changes.insert(path, change);
        }
        let mut prov = provenance(req.provenance.into_option())?;
        merge_provenance(&mut prov, &cv.provenance);
        prov.resolved_by = Some(opened.permit().resolved_by().to_string());
        cv.provenance = prov;
        cv.predecessor = Some(version);
        cv.created_at = Timestamp::now();
        let (hash, cv) = commit_version(&opened, cv).await?;
        Ok(pb::ResolveConflictsResponse {
            info: stored(&opened, &hash).await?.into(),
            version: pb::ChangeVersion::from(cv).into(),
        })
    }
}

/// Where an edit starts from.
struct EditBase {
    parents: Vec<ChangeVersionHash>,
    predecessor: Option<ChangeVersionHash>,
    change_id: Option<ChangeId>,
    root: Option<TreeHash>,
}

/// Resolve an `EditBase`: a point in history (new change on top of it),
/// or an existing change (amend its current version). An explicit
/// `amend` id combined with a root base amends that change onto the
/// given root.
async fn edit_base(
    opened: &Opened,
    base: Option<&pb::EditBase>,
    amend: Option<ChangeId>,
) -> Result<EditBase> {
    let ctx = &opened.repo;
    let current_of = |id: ChangeId| async move {
        let versions = ctx.repo.change_versions(&id).await?;
        versions
            .last()
            .copied()
            .ok_or_else(|| Error::not_found(format!("change {id} has no versions")))
    };
    match base.and_then(|b| b.base.as_ref()) {
        Some(pb::edit_base::Base::Change(name)) => {
            let id = names::change_in(ctx, name)?;
            let current = current_of(id).await?;
            let cv = ctx.derive.change(&current).await?;
            Ok(EditBase {
                parents: cv.parents.clone(),
                predecessor: Some(current),
                change_id: Some(amend.unwrap_or(id)),
                root: Some(common::root_of(ctx, &current).await?),
            })
        }
        other => {
            let revision = match other {
                Some(pb::edit_base::Base::Revision(revision)) => revision.as_str(),
                _ => "",
            };
            let resolved = revision::resolve(ctx, revision).await?;
            let predecessor = match amend {
                Some(id) => Some(current_of(id).await?),
                None => None,
            };
            Ok(EditBase {
                parents: resolved.version.into_iter().collect(),
                predecessor,
                change_id: amend,
                root: Some(resolved.root),
            })
        }
    }
}

async fn set_content(
    objects: &dyn rill_store::ObjectStore,
    set: pb::SetContent,
) -> Result<FileChange> {
    let content = ContentRef::try_from(
        set.content
            .into_option()
            .ok_or(rill_proto::ConvertError::missing("set_content.content"))?,
    )?;
    check_content_exists(objects, &content).await?;
    // Clients may inline any small file; past the threshold it is stored
    // as a blob, like the server stores everything else.
    let content = match content {
        ContentRef::Inline { bytes } if bytes.len() > rill_types::INLINE_THRESHOLD => {
            store_content(objects, bytes).await?
        }
        other => other,
    };
    let kind = rill_proto::convert::file_kind_from_proto("set_content.kind", set.kind)
        .unwrap_or(FileKind::Regular);
    let copy_from = match set.copy_from.into_option() {
        Some(c) => Some(rill_types::objects::change::CopySource::try_from(c)?),
        None => None,
    };
    Ok(FileChange::Set {
        content,
        kind,
        copy_from,
    })
}

fn merge_provenance(target: &mut Provenance, previous: &Provenance) {
    if target.tool.is_none() {
        target.tool = previous.tool.clone();
    }
    if target.model.is_none() {
        target.model = previous.model.clone();
    }
    if target.session.is_none() {
        target.session = previous.session.clone();
    }
    if target.sponsor.is_none() {
        target.sponsor = previous.sponsor.clone();
    }
    for (k, v) in &previous.extra {
        target.extra.entry(k.clone()).or_insert_with(|| v.clone());
    }
}

/// Build a change version from a tree diff, for workspace `NewChange`.
#[allow(clippy::too_many_arguments)]
pub fn version_from_diff(
    entries: Vec<rill_core::TreeDiffEntry>,
    change_id: ChangeId,
    parents: Vec<ChangeVersionHash>,
    predecessor: Option<ChangeVersionHash>,
    author: rill_types::Principal,
    message: String,
    provenance: Provenance,
    intent: Option<IntentId>,
) -> ChangeVersion {
    let file_changes: BTreeMap<RepoPathBuf, FileChange> = entries
        .into_iter()
        .map(|e| {
            let change = match e.after {
                Some((content, kind)) => FileChange::set(content, kind),
                None => FileChange::Delete,
            };
            (e.path, change)
        })
        .collect();
    ChangeVersion {
        change_id,
        parents,
        predecessor,
        subtree_changes: BTreeMap::new(),
        file_changes,
        author,
        provenance,
        intent,
        message,
        created_at: Timestamp::now(),
        extra: BTreeMap::new(),
    }
}
