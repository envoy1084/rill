//! RPCs that write history: creating, editing, rebasing, merging and
//! resolving change versions.

use std::collections::BTreeMap;

use bytes::Bytes;
use rill_core::{merge_changes, rebase, store_content, ChangeBuilder, MergeChangeOptions};
use rill_proto::convert::{hash_to_proto, id_from_proto, opt_id_from_proto};
use rill_proto::rill::v1 as pb;
use rill_review::error::Error as ReviewError;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, ContentRef, FileChange, FileKind, IntentId,
    Provenance, RepoPathBuf, Signature, Timestamp, TreeHash,
};

use super::trees::{check_content_exists, content_bytes};
use super::RepoSvc;
use crate::error::{Error, Result};
use crate::services::common::{self, hash, open, path, provenance, resolve_root, Opened};
use crate::state::RepoEvent;

impl RepoSvc {
    pub(super) async fn create_change_version_impl(
        &self,
        request: tonic::Request<pb::CreateChangeVersionRequest>,
    ) -> Result<pb::CreateChangeVersionResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        let cv = ChangeVersion::try_from(
            req.version
                .ok_or_else(|| Error::invalid("version is required"))?,
        )?;
        for path in cv.file_changes.keys() {
            opened.permit().write(path).await?;
        }
        if cv.file_changes.is_empty() && cv.subtree_changes.is_empty() {
            opened.permit().write(&RepoPathBuf::root()).await?;
        }
        let signatures = req
            .signatures
            .into_iter()
            .map(Signature::try_from)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let (hash, _) = commit_version(&opened, cv).await?;
        if !signatures.is_empty() {
            opened.repo.repo.put_signatures(&hash, &signatures).await?;
        }
        Ok(pb::CreateChangeVersionResponse {
            hash: hash_to_proto(&hash),
        })
    }

    pub(super) async fn apply_edits_impl(
        &self,
        request: tonic::Request<pb::ApplyEditsRequest>,
    ) -> Result<pb::ApplyEditsResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        let ctx = &opened.repo;
        let objects = ctx.repo.objects();

        let amend: Option<ChangeId> = opt_id_from_proto("change_id", req.change_id.as_ref())?;
        let base = edit_base(&opened, req.base.as_ref(), amend).await?;

        let mut builder = ChangeBuilder::new(opened.principal().clone())
            .message(req.message)
            .provenance(provenance(req.provenance)?);
        if let Some(id) = base.change_id {
            builder = builder.change_id(id);
        }
        for parent in &base.parents {
            builder = builder.parent(*parent);
        }
        if let Some(pred) = base.predecessor {
            builder = builder.predecessor(pred);
        }
        if let Some(intent) = opt_id_from_proto::<IntentId>("intent", req.intent.as_ref())? {
            builder = builder.intent(intent);
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
        if cv.file_changes.is_empty() {
            opened.permit().write(&RepoPathBuf::root()).await?;
        }
        let (hash, cv) = commit_version(&opened, cv).await?;
        Ok(pb::ApplyEditsResponse {
            hash: hash_to_proto(&hash),
            version: Some(pb::ChangeVersion::from(cv)),
        })
    }

    pub(super) async fn rebase_impl(
        &self,
        request: tonic::Request<pb::RebaseRequest>,
    ) -> Result<pb::RebaseResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.get_ref();
        opened.permit().write(&RepoPathBuf::root()).await?;
        let version = hash("hash", &req.hash)?;
        let new_parent = hash("new_parent", &req.new_parent)?;
        let rebased = rebase(&opened.repo.derive, &version, &new_parent, Timestamp::now()).await?;
        let (hash, cv) = commit_version(&opened, rebased).await?;
        Ok(pb::RebaseResponse {
            hash: hash_to_proto(&hash),
            version: Some(pb::ChangeVersion::from(cv)),
        })
    }

    pub(super) async fn merge_impl(
        &self,
        request: tonic::Request<pb::MergeRequest>,
    ) -> Result<pb::MergeResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        opened.permit().write(&RepoPathBuf::root()).await?;
        let left = hash("left", &req.left)?;
        let right = hash("right", &req.right)?;
        let mut options = MergeChangeOptions::new(opened.principal().clone(), req.message);
        options.provenance = provenance(req.provenance)?;
        let mut merged = merge_changes(
            &opened.repo.derive,
            &opened.repo.graph,
            &left,
            &right,
            options,
        )
        .await?;
        merged.intent = opt_id_from_proto("intent", req.intent.as_ref())?;
        let (hash, cv) = commit_version(&opened, merged).await?;
        Ok(pb::MergeResponse {
            hash: hash_to_proto(&hash),
            version: Some(pb::ChangeVersion::from(cv)),
        })
    }

    pub(super) async fn resolve_impl(
        &self,
        request: tonic::Request<pb::ResolveRequest>,
    ) -> Result<pb::ResolveResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        let ctx = &opened.repo;
        let version = hash("hash", &req.hash)?;
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
        let mut prov = provenance(req.provenance)?;
        merge_provenance(&mut prov, &cv.provenance);
        prov.resolved_by = Some(opened.permit().resolved_by().to_string());
        cv.provenance = prov;
        cv.predecessor = Some(version);
        cv.created_at = Timestamp::now();
        let (hash, cv) = commit_version(&opened, cv).await?;
        Ok(pb::ResolveResponse {
            hash: hash_to_proto(&hash),
            version: Some(pb::ChangeVersion::from(cv)),
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
        Some(pb::edit_base::Base::ChangeId(bytes)) => {
            let id: ChangeId = id_from_proto("base.change_id", bytes)?;
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
            let spec = match other {
                Some(pb::edit_base::Base::Root(spec)) => Some(spec),
                _ => None,
            };
            let resolved = resolve_root(ctx, spec).await?;
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
            .ok_or_else(|| Error::invalid("set_content.content is required"))?,
    )?;
    check_content_exists(objects, &content).await?;
    let kind = rill_proto::convert::file_kind_from_proto("set_content.kind", set.kind)
        .unwrap_or(FileKind::Regular);
    let copy_from = match set.copy_from {
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

/// Validate and store a change version, tell review about the new
/// version when the change is under review, and publish the event.
pub async fn commit_version(
    opened: &Opened,
    cv: ChangeVersion,
) -> Result<(ChangeVersionHash, ChangeVersion)> {
    let ctx = &opened.repo;
    for parent in &cv.parents {
        if !ctx.repo.has_change_version(parent).await? {
            return Err(Error::precondition(format!(
                "parent {parent} does not exist"
            )));
        }
    }
    if let Some(pred) = &cv.predecessor {
        if !ctx.repo.has_change_version(pred).await? {
            return Err(Error::precondition(format!(
                "predecessor {pred} does not exist"
            )));
        }
    }
    for (path, change) in &cv.file_changes {
        if path.is_root() {
            return Err(Error::invalid("file change at the repository root"));
        }
        if let FileChange::Set { content, .. } = change {
            check_content_exists(ctx.repo.objects().as_ref(), content).await?;
        }
    }
    let hash = ctx.repo.put_change_version(&cv).await?;
    // Materialise the root tree now so a bad change fails the request
    // rather than a later read.
    ctx.derive.get::<rill_core::RootTree>(&hash).await?;
    match ctx.review().add_version(&cv.change_id, hash).await {
        Ok(_) | Err(ReviewError::ReviewNotFound(_)) => {}
        Err(ReviewError::InvalidStatus { status, .. }) => {
            tracing::debug!(change = %cv.change_id, ?status, "review not updated with new version");
        }
        Err(e) => return Err(e.into()),
    }
    ctx.events.publish(RepoEvent::VersionCreated { hash });
    Ok((hash, cv))
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
