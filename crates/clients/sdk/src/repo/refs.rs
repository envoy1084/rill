//! Refs and trunk: resolution, listing, compare-and-swap moves, head and
//! ranges.

use rill_proto::convert::{
    hash_from_proto, id_to_proto, opt_hash_from_proto, trunk_seq_from_proto,
};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersionHash, OperationId, RefName, RefUpdate, TrunkSeq};

use super::RepoClient;
use crate::error::{Error, Result};
use crate::types::{LandEvent, RefInfo, Root};

impl RepoClient {
    /// The target of a ref, with its trunk sequence when it is trunk.
    pub async fn resolve_ref(
        &self,
        name: &RefName,
    ) -> Result<(ChangeVersionHash, Option<TrunkSeq>)> {
        let resp = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::ResolveRefRequest {
                repo: self.repo.clone(),
                name: name.to_string(),
            };
            async move { svc.resolve_ref(request).await }
        })
        .await?
        .into_inner();
        Ok((
            hash_from_proto("resolve_ref.hash", &resp.hash)?,
            resp.trunk_seq.map(trunk_seq_from_proto),
        ))
    }

    /// Resolve a root to a change version hash. A tree root has no
    /// version and fails with `InvalidArgument`.
    pub async fn resolve_root(&self, root: &Root) -> Result<ChangeVersionHash> {
        match root {
            Root::Version(h) => Ok(*h),
            Root::Ref(name) => Ok(self.resolve_ref(name).await?.0),
            Root::Seq(seq) => {
                let mut landings = self.trunk_range(*seq, *seq).await?;
                landings.pop().map(|l| l.hash).ok_or_else(|| Error::Status {
                    code: tonic::Code::NotFound,
                    message: format!("{seq} is not on trunk"),
                })
            }
            Root::Tree(h) => Err(Error::Status {
                code: tonic::Code::InvalidArgument,
                message: format!("tree {h} is not a change version"),
            }),
        }
    }

    /// Refs whose names start with `prefix` (all refs when `None`).
    pub async fn list_refs(&self, prefix: Option<&str>) -> Result<Vec<RefInfo>> {
        let resp = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::ListRefsRequest {
                repo: self.repo.clone(),
                prefix: prefix.map(str::to_string),
            };
            async move { svc.list_refs(request).await }
        })
        .await?
        .into_inner();
        resp.refs.into_iter().map(RefInfo::try_from).collect()
    }

    /// Cut `release/<name>` at trunk position `at` (the head when `None`).
    /// Returns the ref, the version it points at and that trunk position.
    pub async fn cut_release(
        &self,
        name: &str,
        at: Option<TrunkSeq>,
    ) -> Result<(RefName, ChangeVersionHash, TrunkSeq)> {
        let request = pb::CutReleaseRequest {
            operation_id: id_to_proto(&OperationId::new()),
            repo: self.repo.clone(),
            name: name.to_string(),
            at: at.map(|s| s.get()),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.cut_release(request).await }
        })
        .await?
        .into_inner();
        Ok((
            RefName::new(resp.r#ref)?,
            hash_from_proto("cut_release.hash", &resp.hash)?,
            TrunkSeq(resp.trunk_seq),
        ))
    }

    /// Compare-and-swap a ref: `expected` is the required current target
    /// (`None` means the ref must not exist), `target` the new one (`None`
    /// deletes). Returns the target after the move.
    pub async fn move_ref(
        &self,
        name: RefName,
        expected: Option<ChangeVersionHash>,
        target: Option<ChangeVersionHash>,
    ) -> Result<Option<ChangeVersionHash>> {
        self.move_ref_with_op(OperationId::new(), name, expected, target)
            .await
    }

    /// [`RepoClient::move_ref`] with a caller-chosen idempotency key.
    pub async fn move_ref_with_op(
        &self,
        op: OperationId,
        name: RefName,
        expected: Option<ChangeVersionHash>,
        target: Option<ChangeVersionHash>,
    ) -> Result<Option<ChangeVersionHash>> {
        let update = RefUpdate {
            name,
            expected,
            target,
        };
        let request = pb::MoveRefRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            update: Some(pb::RefUpdate::from(update)),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.move_ref(request).await }
        })
        .await?
        .into_inner();
        Ok(opt_hash_from_proto("move_ref.hash", resp.hash.as_ref())?)
    }

    /// The head of trunk, or `None` for an empty repository.
    pub async fn trunk_head(&self) -> Result<Option<(TrunkSeq, ChangeVersionHash)>> {
        let resp = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::TrunkHeadRequest {
                repo: self.repo.clone(),
            };
            async move { svc.trunk_head(request).await }
        })
        .await?
        .into_inner();
        if resp.hash.is_empty() {
            return Ok(None);
        }
        Ok(Some((
            trunk_seq_from_proto(resp.seq),
            hash_from_proto("trunk_head.hash", &resp.hash)?,
        )))
    }

    /// Landings between two sequence numbers, inclusive.
    pub async fn trunk_range(&self, from: TrunkSeq, to: TrunkSeq) -> Result<Vec<LandEvent>> {
        let resp = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::TrunkRangeRequest {
                repo: self.repo.clone(),
                from: from.get(),
                to: to.get(),
            };
            async move { svc.trunk_range(request).await }
        })
        .await?
        .into_inner();
        resp.landings.into_iter().map(LandEvent::try_from).collect()
    }
}
