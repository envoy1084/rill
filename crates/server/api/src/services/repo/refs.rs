//! Refs, releases, trunk positions and derived values.

use rill_proto::convert::hash_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersionHash, ObjectHash, RefName, RepoPathBuf, TrunkSeq};

use super::RepoSvc;
use crate::listing::{Filter, Kind, Page, Position};
use crate::services::common::{self, hash, open_repo};
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

/// Longest a `WaitDerivedValue` call blocks.
const MAX_WAIT_MS: u64 = 60_000;
const WAIT_POLL: std::time::Duration = std::time::Duration::from_millis(50);
/// Trunk positions read at a time while filling a page.
const TRUNK_SCAN: u64 = 1000;

/// A ref's etag: its target, so an update fenced on it is a true
/// compare-and-swap.
fn etag(target: &ChangeVersionHash) -> String {
    target.to_hex()
}

/// The wire form of `short` pointing at `target`.
async fn ref_proto(
    ctx: &RepoContext,
    short: &RefName,
    target: ChangeVersionHash,
) -> Result<pb::Ref> {
    let trunk_position = if short.is_trunk() {
        ctx.repo.trunk_seq_of(&target).await?.map(TrunkSeq::get)
    } else {
        None
    };
    Ok(pb::Ref {
        name: names::ref_name(ctx, short.as_str()),
        target: hash_to_proto(&target),
        trunk_position,
        etag: etag(&target),
    })
}

/// The short ref name a ref resource name holds.
fn short_name(name: &str) -> Result<RefName> {
    let short = names::parse(&rill_proto::names::REF, name)?[2];
    RefName::new(short).map_err(|e| Error::InvalidName(format!("{short:?}: {e}")))
}

impl RepoSvc {
    pub(super) async fn get_ref_impl(
        &self,
        request: Call<pb::GetRefRequest>,
    ) -> Result<pb::GetRefResponse> {
        let name = &request.get_ref().name;
        let short = short_name(name)?;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().read_any().await?;
        let target = opened
            .repo
            .repo
            .get_ref(&short)
            .await?
            .ok_or_else(|| Error::not_found(format!("ref {short}")))?;
        Ok(pb::GetRefResponse {
            r#ref: ref_proto(&opened.repo, &short, target).await?.into(),
        })
    }

    pub(super) async fn list_refs_impl(
        &self,
        request: Call<pb::ListRefsRequest>,
    ) -> Result<pb::ListRefsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read_any().await?;
        let filter = Filter::parse(&req.filter, &[("name", Kind::Str)])?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.filter])?;
        let mut refs: Vec<_> = opened
            .repo
            .repo
            .list_refs()
            .await?
            .into_iter()
            .filter(|(name, _)| filter.matches(&NamedRef(name.as_str())))
            .collect();
        refs.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        let (refs, next_page_token) = page.of(refs)?;
        let mut out = Vec::with_capacity(refs.len());
        for (name, target) in refs {
            out.push(ref_proto(&opened.repo, &name, target).await?);
        }
        Ok(pb::ListRefsResponse {
            refs: out,
            next_page_token,
        })
    }

    /// Compare-and-swap a ref on its etag. Trunk and releases are owned by
    /// their landers: they only move through landings.
    pub(super) async fn update_ref_impl(
        &self,
        request: Call<pb::UpdateRefRequest>,
    ) -> Result<pb::UpdateRefResponse> {
        let req = request.get_ref();
        let wire = req
            .r#ref
            .as_option()
            .ok_or(rill_proto::ConvertError::missing("ref"))?;
        let short = short_name(&wire.name)?;
        let opened = open_repo(&self.state, &request, &wire.name).await?;
        opened.permit().write(&RepoPathBuf::root()).await?;
        refuse_lander_refs(&short)?;
        let target = hash("ref.target", &wire.target)?;
        if !opened.repo.repo.has_change_version(&target).await? {
            return Err(Error::MissingObjects(format!(
                "version {target} does not exist"
            )));
        }
        let expected = match (req.etag.as_str(), req.allow_missing) {
            ("", true) => None,
            ("", false) => {
                return Err(Error::invalid(
                    "etag is required to update a ref; set allow_missing to create one",
                ))
            }
            (etag, _) => Some(parse_etag(etag)?),
        };
        opened
            .repo
            .repo
            .update_ref(&short, expected, Some(target), opened.principal())
            .await
            .map_err(|e| moved(e, &short))?;
        Ok(pb::UpdateRefResponse {
            r#ref: ref_proto(&opened.repo, &short, target).await?.into(),
        })
    }

    pub(super) async fn delete_ref_impl(
        &self,
        request: Call<pb::DeleteRefRequest>,
    ) -> Result<pb::DeleteRefResponse> {
        let req = request.get_ref();
        let short = short_name(&req.name)?;
        let opened = open_repo(&self.state, &request, &req.name).await?;
        opened.permit().write(&RepoPathBuf::root()).await?;
        refuse_lander_refs(&short)?;
        if req.etag.is_empty() {
            return Err(Error::invalid("etag is required to delete a ref"));
        }
        opened
            .repo
            .repo
            .update_ref(
                &short,
                Some(parse_etag(&req.etag)?),
                None,
                opened.principal(),
            )
            .await
            .map_err(|e| moved(e, &short))?;
        Ok(pb::DeleteRefResponse {})
    }

    /// Cut `release/{release_id}` at a trunk position: owners only.
    pub(super) async fn create_release_impl(
        &self,
        request: Call<pb::CreateReleaseRequest>,
    ) -> Result<pb::CreateReleaseResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().admin().await?;
        let release = &req.release_id;
        let ref_name = RefName::release(release)
            .map_err(|e| Error::invalid(format!("release name {release:?}: {e}")))?;
        let repo = &opened.repo.repo;
        let (_, hash) = match req.trunk_position {
            Some(at) => {
                let hash = repo
                    .trunk_at(TrunkSeq(at))
                    .await?
                    .ok_or_else(|| Error::not_found(format!("trunk@{at}")))?;
                (TrunkSeq(at), hash)
            }
            None => repo
                .trunk_head()
                .await?
                .ok_or_else(|| Error::precondition("trunk is empty; nothing to cut"))?,
        };
        repo.cut_release(release, hash, opened.principal())
            .await
            .map_err(|e| match e.is_conflict() {
                true => Error::AlreadyExists(format!("{ref_name} already exists")),
                false => e.into(),
            })?;
        Ok(pb::CreateReleaseResponse {
            r#ref: ref_proto(&opened.repo, &ref_name, hash).await?.into(),
        })
    }

    pub(super) async fn get_trunk_position_impl(
        &self,
        request: Call<pb::GetTrunkPositionRequest>,
    ) -> Result<pb::GetTrunkPositionResponse> {
        let name = &request.get_ref().name;
        let position = names::parse(&rill_proto::names::TRUNK_POSITION, name)?[2].to_string();
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().read_any().await?;
        let repo = &opened.repo.repo;
        let (seq, hash) = match position.as_str() {
            "head" => repo
                .trunk_head()
                .await?
                .ok_or_else(|| Error::not_found("trunk has no head yet"))?,
            n => {
                let seq = TrunkSeq(n.parse().map_err(|_| {
                    Error::InvalidName(format!("{n:?} is not a trunk position or head"))
                })?);
                let hash = repo
                    .trunk_at(seq)
                    .await?
                    .ok_or_else(|| Error::not_found(format!("trunk@{}", seq.get())))?;
                (seq, hash)
            }
        };
        Ok(pb::GetTrunkPositionResponse {
            landing: common::landing(&opened.repo, seq, hash).await?.into(),
        })
    }

    /// Trunk positions oldest first, read by key so a page costs its size.
    pub(super) async fn list_trunk_positions_impl(
        &self,
        request: Call<pb::ListTrunkPositionsRequest>,
    ) -> Result<pb::ListTrunkPositionsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read_any().await?;
        let filter = Filter::parse(&req.filter, &[("position", Kind::Int)])?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.filter])?;
        let mut after = match &page.start {
            Position::Start => 0,
            Position::After(key) => u64::from_be_bytes(
                key.as_slice()
                    .try_into()
                    .map_err(|_| Error::InvalidPageToken("not a token for this list".into()))?,
            ),
            Position::Offset(_) => {
                return Err(Error::InvalidPageToken("not a token for this list".into()))
            }
        };
        let repo = &opened.repo.repo;
        let head = repo.trunk_head().await?.map(|(s, _)| s.get()).unwrap_or(0);
        let mut landings = Vec::new();
        while landings.len() < page.size && after < head {
            let to = (after + TRUNK_SCAN).min(head);
            for (seq, hash) in repo.trunk_range(TrunkSeq(after), TrunkSeq(to)).await? {
                after = seq.get();
                if filter.matches(&TrunkRecord(seq.get())) {
                    landings.push(common::landing(&opened.repo, seq, hash).await?);
                    if landings.len() == page.size {
                        break;
                    }
                }
            }
            if after < to && landings.len() < page.size {
                after = to;
            }
        }
        let next_page_token = if after < head && landings.len() == page.size {
            page.token(Position::After(after.to_be_bytes().to_vec()))
        } else {
            String::new()
        };
        Ok(pb::ListTrunkPositionsResponse {
            landings,
            next_page_token,
        })
    }

    pub(super) async fn get_derived_value_impl(
        &self,
        request: Call<pb::GetDerivedValueRequest>,
    ) -> Result<pb::GetDerivedValueResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let req = request.get_ref();
        let hash = hash("hash", &req.hash)?;
        match derived(&opened.repo, &req.kind, &hash).await? {
            Some(data) => Ok(pb::GetDerivedValueResponse { data }),
            None => Err(Error::not_found(format!(
                "derived {:?} for {hash}",
                req.kind
            ))),
        }
    }

    pub(super) async fn wait_derived_value_impl(
        &self,
        request: Call<pb::WaitDerivedValueRequest>,
    ) -> Result<pb::WaitDerivedValueResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let req = request.get_ref();
        let hash = hash("hash", &req.hash)?;
        let deadline = tokio::time::Instant::now()
            + std::time::Duration::from_millis(req.timeout_ms.min(MAX_WAIT_MS));
        loop {
            if let Some(data) = derived(&opened.repo, &req.kind, &hash).await? {
                return Ok(pb::WaitDerivedValueResponse {
                    ready: true,
                    data: Some(data),
                });
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(pb::WaitDerivedValueResponse {
                    ready: false,
                    data: None,
                });
            }
            tokio::time::sleep(WAIT_POLL).await;
        }
    }
}

/// Trunk and releases move only through their landers.
fn refuse_lander_refs(name: &RefName) -> Result<()> {
    if name.is_trunk() {
        return Err(Error::denied(
            "trunk only moves through landings; use LandingService.RequestLand",
        ));
    }
    if name.is_release() {
        return Err(Error::denied(
            "a release only moves through its lander: cut it with CreateRelease \
             and land on it with RequestLand onto it",
        ));
    }
    Ok(())
}

fn parse_etag(etag: &str) -> Result<ChangeVersionHash> {
    etag.parse()
        .map_err(|_| Error::EtagMismatch(format!("{etag:?} is not this ref's etag")))
}

/// A lost compare-and-swap is a stale etag.
fn moved(e: rill_store::Error, name: &RefName) -> Error {
    if e.is_conflict() {
        Error::EtagMismatch(format!("ref {name} has moved; read it again"))
    } else {
        e.into()
    }
}

struct NamedRef<'a>(&'a str);

impl crate::listing::Record for NamedRef<'_> {
    fn field(&self, _: &str) -> crate::listing::FieldValue {
        crate::listing::FieldValue::Str(self.0.to_string())
    }
}

struct TrunkRecord(u64);

impl crate::listing::Record for TrunkRecord {
    fn field(&self, _: &str) -> crate::listing::FieldValue {
        crate::listing::FieldValue::Int(self.0 as i64)
    }
}

/// A derived record's raw bytes, computed on demand for the kinds the
/// framework knows.
async fn derived(ctx: &RepoContext, kind: &str, hash: &ObjectHash) -> Result<Option<bytes::Bytes>> {
    if ctx.repo.has_change_version(hash).await? {
        ctx.derive.compute_kind(kind, hash).await?;
    }
    Ok(ctx.repo.get_derived(kind, hash).await?)
}
