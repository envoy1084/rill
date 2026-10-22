//! History writes: new versions, edits, rebase, merge and resolution.

use rill_proto::convert::{hash_from_proto, hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersion, ChangeVersionHash, OperationId, Provenance, Signature};

use super::blobs::stored_set;
use super::RepoClient;
use crate::error::Result;
use crate::types::{ApplyOptions, Edit, EditBase, MergeOptions, Resolution, VersionResult};

impl RepoClient {
    /// Store a fully built change version. Returns its hash.
    pub async fn create_change_version(
        &self,
        version: &ChangeVersion,
        signatures: Vec<Signature>,
    ) -> Result<ChangeVersionHash> {
        self.create_change_version_with_op(OperationId::new(), version, signatures)
            .await
    }

    /// [`RepoClient::create_change_version`] with a caller-chosen
    /// idempotency key.
    pub async fn create_change_version_with_op(
        &self,
        op: OperationId,
        version: &ChangeVersion,
        signatures: Vec<Signature>,
    ) -> Result<ChangeVersionHash> {
        let request = pb::CreateChangeVersionRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            version: pb::ChangeVersion::from(version).into(),
            signatures: signatures.into_iter().map(pb::Signature::from).collect(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_change_version(request).await }
        })
        .await?
        .into_owned();
        Ok(hash_from_proto("create_change_version.hash", &resp.hash)?)
    }

    /// Apply edits on top of `base`, producing a new version. Large file
    /// contents are uploaded as blobs first.
    pub async fn apply_edits(
        &self,
        base: EditBase,
        edits: Vec<Edit>,
        opts: ApplyOptions,
    ) -> Result<VersionResult> {
        self.apply_edits_with_op(OperationId::new(), base, edits, opts)
            .await
    }

    /// [`RepoClient::apply_edits`] with a caller-chosen idempotency key.
    pub async fn apply_edits_with_op(
        &self,
        op: OperationId,
        base: EditBase,
        edits: Vec<Edit>,
        opts: ApplyOptions,
    ) -> Result<VersionResult> {
        let mut wire = Vec::with_capacity(edits.len());
        for edit in edits {
            wire.push(self.edit_to_proto(edit).await?);
        }
        let request = pb::ApplyEditsRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            base: base.to_proto().into(),
            edits: wire,
            message: opts.message,
            intent: opts.intent.as_ref().map(id_to_proto),
            provenance: pb::Provenance::from(&opts.provenance).into(),
            change_id: opts.change_id.as_ref().map(id_to_proto),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.apply_edits(request).await }
        })
        .await?
        .into_owned();
        VersionResult::from_parts(&resp.hash, resp.version.into_option())
    }

    async fn edit_to_proto(&self, edit: Edit) -> Result<pb::Edit> {
        let (path, op) = match edit {
            Edit::Set { path, bytes, kind } => (
                path,
                pb::edit::Op::SetContent(stored_set(self, bytes, kind).await?),
            ),
            Edit::Delete { path } => (path, pb::edit::Op::Delete(true)),
            Edit::Patch { path, unified } => (path, pb::edit::Op::Patch(unified)),
        };
        Ok(pb::Edit {
            path: path.to_string(),
            op: Some(op),
        })
    }

    /// Rebase a version onto a new parent. Conflicts are recorded in the
    /// result, never returned as an error.
    pub async fn rebase(
        &self,
        hash: ChangeVersionHash,
        new_parent: ChangeVersionHash,
    ) -> Result<VersionResult> {
        self.rebase_with_op(OperationId::new(), hash, new_parent)
            .await
    }

    /// [`RepoClient::rebase`] with a caller-chosen idempotency key.
    pub async fn rebase_with_op(
        &self,
        op: OperationId,
        hash: ChangeVersionHash,
        new_parent: ChangeVersionHash,
    ) -> Result<VersionResult> {
        let request = pb::RebaseRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            hash: hash_to_proto(&hash),
            new_parent: hash_to_proto(&new_parent),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.rebase(request).await }
        })
        .await?
        .into_owned();
        VersionResult::from_parts(&resp.hash, resp.version.into_option())
    }

    /// Merge two versions into a new change.
    pub async fn merge(
        &self,
        left: ChangeVersionHash,
        right: ChangeVersionHash,
        opts: MergeOptions,
    ) -> Result<VersionResult> {
        self.merge_with_op(OperationId::new(), left, right, opts)
            .await
    }

    /// [`RepoClient::merge`] with a caller-chosen idempotency key.
    pub async fn merge_with_op(
        &self,
        op: OperationId,
        left: ChangeVersionHash,
        right: ChangeVersionHash,
        opts: MergeOptions,
    ) -> Result<VersionResult> {
        let request = pb::MergeRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            left: hash_to_proto(&left),
            right: hash_to_proto(&right),
            message: opts.message,
            intent: opts.intent.as_ref().map(id_to_proto),
            provenance: pb::Provenance::from(&opts.provenance).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.merge(request).await }
        })
        .await?
        .into_owned();
        VersionResult::from_parts(&resp.hash, resp.version.into_option())
    }

    /// Resolve conflicted paths of a version, producing a new version.
    pub async fn resolve(
        &self,
        hash: ChangeVersionHash,
        resolutions: Vec<Resolution>,
        provenance: Provenance,
    ) -> Result<VersionResult> {
        self.resolve_with_op(OperationId::new(), hash, resolutions, provenance)
            .await
    }

    /// [`RepoClient::resolve`] with a caller-chosen idempotency key.
    pub async fn resolve_with_op(
        &self,
        op: OperationId,
        hash: ChangeVersionHash,
        resolutions: Vec<Resolution>,
        provenance: Provenance,
    ) -> Result<VersionResult> {
        let mut wire = Vec::with_capacity(resolutions.len());
        for r in resolutions {
            let (path, resolution) = match r {
                Resolution::Set { path, bytes, kind } => (
                    path,
                    pb::resolution::Resolution::Set(stored_set(self, bytes, kind).await?),
                ),
                Resolution::Delete { path } => (path, pb::resolution::Resolution::Delete(true)),
            };
            wire.push(pb::Resolution {
                path: path.to_string(),
                resolution: Some(resolution),
            });
        }
        let request = pb::ResolveRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            hash: hash_to_proto(&hash),
            resolutions: wire,
            provenance: pb::Provenance::from(&provenance).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.resolve(request).await }
        })
        .await?
        .into_owned();
        VersionResult::from_parts(&resp.hash, resp.version.into_option())
    }
}
