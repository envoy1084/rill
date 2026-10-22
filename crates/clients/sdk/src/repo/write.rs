//! History writes: new versions, edits, rebase, merge and resolution.

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
        let request = pb::CreateVersionRequest {
            request_id: crate::names::request_id(&op),
            parent: self.resource.clone(),
            version: pb::ChangeVersion::from(version).into(),
            signatures: signatures.into_iter().map(pb::Signature::from).collect(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.changes(), request.clone());
            async move { svc.create_version(request).await }
        })
        .await?
        .into_owned();
        let info = rill_proto::convert::required("create_version.version", resp.version)?;
        Ok(rill_proto::convert::hash_from_proto(
            "version.hash",
            &info.hash,
        )?)
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
            request_id: crate::names::request_id(&op),
            parent: self.resource.clone(),
            base: base.to_proto(&self.resource).into(),
            edits: wire,
            message: opts.message,
            intent: opts
                .intent
                .map(|i| crate::names::intent(&self.resource, &i))
                .unwrap_or_default(),
            provenance: pb::Provenance::from(&opts.provenance).into(),
            change: opts
                .change_id
                .map(|c| crate::names::change(&self.resource, &c))
                .unwrap_or_default(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.changes(), request.clone());
            async move { svc.apply_edits(request).await }
        })
        .await?
        .into_owned();
        VersionResult::from_parts(resp.info.into_option(), resp.version.into_option())
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
        let request = pb::RebaseVersionRequest {
            request_id: crate::names::request_id(&op),
            name: crate::names::version(&self.resource, &hash),
            onto: new_parent.to_hex(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.changes(), request.clone());
            async move { svc.rebase_version(request).await }
        })
        .await?
        .into_owned();
        VersionResult::from_parts(resp.info.into_option(), resp.version.into_option())
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
        let request = pb::MergeVersionsRequest {
            request_id: crate::names::request_id(&op),
            parent: self.resource.clone(),
            left: left.to_hex(),
            right: right.to_hex(),
            message: opts.message,
            intent: opts
                .intent
                .map(|i| crate::names::intent(&self.resource, &i))
                .unwrap_or_default(),
            provenance: pb::Provenance::from(&opts.provenance).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.changes(), request.clone());
            async move { svc.merge_versions(request).await }
        })
        .await?
        .into_owned();
        VersionResult::from_parts(resp.info.into_option(), resp.version.into_option())
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
        let request = pb::ResolveConflictsRequest {
            request_id: crate::names::request_id(&op),
            name: crate::names::version(&self.resource, &hash),
            resolutions: wire,
            provenance: pb::Provenance::from(&provenance).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.changes(), request.clone());
            async move { svc.resolve_conflicts(request).await }
        })
        .await?
        .into_owned();
        VersionResult::from_parts(resp.info.into_option(), resp.version.into_option())
    }
}
