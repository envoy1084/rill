//! Refs and trunk: resolution, listing, compare-and-swap moves, head and
//! ranges.

use rill_proto::convert::{hash_from_proto, hash_to_proto, required, trunk_seq_from_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersionHash, OperationId, RefName, TrunkSeq};

use super::RepoClient;
use crate::error::{Error, Result};
use crate::types::{LandEvent, RefInfo, Root};

impl RepoClient {
    /// The target of a ref, with its trunk position when it is trunk.
    pub async fn resolve_ref(
        &self,
        name: &RefName,
    ) -> Result<(ChangeVersionHash, Option<TrunkSeq>)> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetRefRequest {
                name: crate::names::ref_name(&self.resource, name),
            };
            async move { svc.get_ref(request).await }
        })
        .await?
        .into_owned();
        let found = required("get_ref.ref", resp.r#ref)?;
        Ok((
            hash_from_proto("ref.target", &found.target)?,
            found.trunk_position.map(trunk_seq_from_proto),
        ))
    }

    /// Resolve a root to a change version hash. A tree root has no
    /// version and fails with `InvalidArgument`.
    pub async fn resolve_root(&self, root: &Root) -> Result<ChangeVersionHash> {
        if let Root::Version(h) = root {
            return Ok(*h);
        }
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::ResolveRevisionRequest {
                repo: self.resource.clone(),
                revision: root.to_string(),
            };
            async move { svc.resolve_revision(request).await }
        })
        .await?
        .into_owned();
        if resp.version_hash.is_empty() {
            return Err(Error::status(
                connectrpc::ErrorCode::InvalidArgument,
                format!("{root} is not a change version"),
            ));
        }
        Ok(hash_from_proto("version_hash", &resp.version_hash)?)
    }

    /// Refs whose names start with `prefix` (all refs when `None`).
    pub async fn list_refs(&self, prefix: Option<&str>) -> Result<Vec<RefInfo>> {
        let filter = match prefix {
            Some(p) if !p.is_empty() => format!("name = {:?}", format!("{p}*")),
            _ => String::new(),
        };
        crate::paging::all(|page_token| {
            let filter = filter.clone();
            async move {
                let resp = crate::client::retry_idempotent(|| {
                    let svc = self.svc();
                    let request = pb::ListRefsRequest {
                        parent: self.resource.clone(),
                        page_size: crate::paging::PAGE,
                        page_token: page_token.clone(),
                        filter: filter.clone(),
                    };
                    async move { svc.list_refs(request).await }
                })
                .await?
                .into_owned();
                let refs = resp
                    .refs
                    .into_iter()
                    .map(RefInfo::try_from)
                    .collect::<Result<Vec<_>>>()?;
                Ok((refs, resp.next_page_token))
            }
        })
        .await
    }

    /// Cut `release/<name>` at trunk position `at` (the head when `None`).
    /// Returns the ref, the version it points at and that trunk position.
    pub async fn cut_release(
        &self,
        name: &str,
        at: Option<TrunkSeq>,
    ) -> Result<(RefName, ChangeVersionHash, TrunkSeq)> {
        let request = pb::CreateReleaseRequest {
            request_id: crate::names::request_id(&OperationId::new()),
            parent: self.resource.clone(),
            release_id: name.to_string(),
            trunk_position: at.map(|s| s.get()),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_release(request).await }
        })
        .await?
        .into_owned();
        let cut = required("create_release.ref", resp.r#ref)?;
        let target = hash_from_proto("ref.target", &cut.target)?;
        let seq = match at {
            Some(seq) => seq,
            None => self.trunk_head().await?.map(|(s, _)| s).unwrap_or_default(),
        };
        Ok((crate::names::ref_short(&cut.name)?, target, seq))
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
        let request_id = crate::names::request_id(&op);
        let ref_name = crate::names::ref_name(&self.resource, &name);
        // A ref's etag is its target.
        let etag = expected.map(|h| h.to_hex()).unwrap_or_default();
        match target {
            Some(target) => {
                let request = pb::UpdateRefRequest {
                    request_id,
                    r#ref: pb::Ref {
                        name: ref_name,
                        target: hash_to_proto(&target),
                        ..Default::default()
                    }
                    .into(),
                    allow_missing: expected.is_none(),
                    etag,
                };
                crate::client::retry_idempotent(|| {
                    let (svc, request) = (self.svc(), request.clone());
                    async move { svc.update_ref(request).await }
                })
                .await?;
            }
            None => {
                let request = pb::DeleteRefRequest {
                    request_id,
                    name: ref_name,
                    etag,
                };
                crate::client::retry_idempotent(|| {
                    let (svc, request) = (self.svc(), request.clone());
                    async move { svc.delete_ref(request).await }
                })
                .await?;
            }
        }
        Ok(target)
    }

    /// The head of trunk, or `None` for an empty repository.
    pub async fn trunk_head(&self) -> Result<Option<(TrunkSeq, ChangeVersionHash)>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetTrunkPositionRequest {
                name: format!("{}/trunk/positions/head", self.resource),
            };
            async move { svc.get_trunk_position(request).await }
        })
        .await;
        let resp = match resp {
            Ok(r) => r.into_owned(),
            Err(e) if e.code == connectrpc::ErrorCode::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let landing = LandEvent::try_from(required("landing", resp.landing)?)?;
        Ok(Some((landing.seq, landing.hash)))
    }

    /// Landings between two trunk positions, inclusive.
    pub async fn trunk_range(&self, from: TrunkSeq, to: TrunkSeq) -> Result<Vec<LandEvent>> {
        let filter = format!("position >= {} AND position <= {}", from.get(), to.get());
        crate::paging::all(|page_token| {
            let filter = filter.clone();
            async move {
                let resp = crate::client::retry_idempotent(|| {
                    let svc = self.svc();
                    let request = pb::ListTrunkPositionsRequest {
                        parent: self.resource.clone(),
                        page_size: crate::paging::PAGE,
                        page_token: page_token.clone(),
                        filter: filter.clone(),
                    };
                    async move { svc.list_trunk_positions(request).await }
                })
                .await?
                .into_owned();
                let landings = resp
                    .landings
                    .into_iter()
                    .map(LandEvent::try_from)
                    .collect::<Result<Vec<_>>>()?;
                Ok((landings, resp.next_page_token))
            }
        })
        .await
    }
}
