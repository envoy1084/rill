//! Helpers shared by every service: opening a request (caller plus
//! repository) and building the wire records that several services return.

use std::sync::Arc;

use rill_core::RootTree;
use rill_lander::{landing_of, LandStatus, LandTicket};
use rill_policy::AuthContext;
use rill_proto::convert::{hash_from_proto, hash_to_proto, id_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersionHash, Principal, Provenance, RepoPathBuf, TreeHash, TrunkSeq};

use crate::auth::{caller, Permit};
use crate::services::Call;
use connectrpc::{ConnectError, Response};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::{AppState, RepoContext};

/// What a handler returns.
pub type GrpcResult<T> = std::result::Result<Response<T>, ConnectError>;

/// A request's caller and the repository it names.
pub struct Opened {
    /// The authenticated caller.
    pub ctx: AuthContext,
    /// The repository context.
    pub repo: Arc<RepoContext>,
    dev_mode: bool,
}

impl Opened {
    /// Permission checks for this caller on this repository.
    pub fn permit(&self) -> Permit<'_> {
        Permit::new(&self.ctx, &self.repo, self.dev_mode)
    }

    /// The calling principal.
    pub fn principal(&self) -> &Principal {
        &self.ctx.principal
    }

    /// Whether the server runs without access control.
    pub fn dev_mode(&self) -> bool {
        self.dev_mode
    }
}

/// Resolve the caller and the repository named by `repo` (`org/name`).
async fn open<T>(state: &AppState, request: &Call<T>, repo: &str) -> Result<Opened> {
    let ctx = caller(request)?;
    let repo = state.repos.resolve(repo).await?;
    Ok(Opened {
        ctx,
        repo,
        dev_mode: state.dev_mode(),
    })
}

/// Build an [`Opened`] for a repository context already in hand.
pub fn opened(state: &AppState, ctx: AuthContext, repo: Arc<RepoContext>) -> Opened {
    Opened {
        ctx,
        repo,
        dev_mode: state.dev_mode(),
    }
}

/// The root tree of a version.
pub async fn root_of(ctx: &RepoContext, version: &ChangeVersionHash) -> Result<TreeHash> {
    Ok(ctx.derive.get::<RootTree>(version).await?)
}

/// Decode a required 32-byte hash field.
pub fn hash(field: &'static str, bytes: &[u8]) -> Result<ChangeVersionHash> {
    Ok(hash_from_proto(field, bytes)?)
}

/// Decode a path field.
pub fn path(field: &'static str, value: &str) -> Result<RepoPathBuf> {
    Ok(rill_proto::convert::path_from_proto(field, value)?)
}

/// Resolve the caller and the repository a resource name is in.
pub async fn open_repo<T>(state: &AppState, request: &Call<T>, name: &str) -> Result<Opened> {
    let spec = super::names::repo_spec(name)?;
    open(state, request, &spec).await
}

/// Client-supplied provenance, with the fields only the server may set
/// cleared: `sponsor` and `resolved_by` feed risk and submit policy, so a
/// client claiming them could lower its own review bar.
pub fn provenance(value: Option<pb::Provenance>) -> Result<Provenance> {
    let mut provenance = match value {
        Some(p) => Provenance::try_from(p)?,
        None => Provenance::default(),
    };
    clear_server_fields(&mut provenance);
    Ok(provenance)
}

/// Clear the provenance fields only the server may set.
pub fn clear_server_fields(provenance: &mut Provenance) {
    provenance.sponsor = None;
    provenance.resolved_by = None;
}

/// The wire record for a repository.
pub async fn repo_proto(ctx: &RepoContext) -> Result<pb::Repo> {
    let head = ctx.repo.trunk_head().await?;
    Ok(pb::Repo {
        name: super::names::repo(ctx),
        uid: id_to_proto(&ctx.config.id),
        repo_id: ctx.config.name.clone(),
        description: (!ctx.config.description.is_empty()).then(|| ctx.config.description.clone()),
        created_at: timestamp_to_proto(ctx.config.created_at),
        trunk_head_position: head.map(|(s, _)| s.get()).unwrap_or(0),
        windows_safe: ctx.config.settings.windows_safe,
        org: ctx.config.org.clone(),
    })
}

/// The wire record for a change version's metadata.
pub async fn change_info(ctx: &RepoContext, hash: &ChangeVersionHash) -> Result<pb::ChangeInfo> {
    let info = ctx
        .repo
        .get_change_info(hash)
        .await?
        .ok_or_else(|| Error::not_found(format!("change version {hash}")))?;
    Ok(change_info_proto(ctx, &info))
}

/// The wire record for a version's metadata, with its resource name.
pub fn change_info_proto(ctx: &RepoContext, info: &rill_types::ChangeInfo) -> pb::ChangeInfo {
    pb::ChangeInfo {
        name: super::names::version(ctx, &info.hash),
        ..pb::ChangeInfo::from(info)
    }
}

/// The wire record for the landing at `seq`.
pub async fn landing(
    ctx: &RepoContext,
    seq: TrunkSeq,
    version: ChangeVersionHash,
) -> Result<pb::Landing> {
    let cv = ctx.derive.change(&version).await?;
    let landing = landing_of(&ctx.repo, &version).await?;
    let (landed_by, at) = match &landing {
        Some(l) => (l.requester.clone(), l.landed_at),
        None => (cv.author.clone(), cv.created_at),
    };
    let reverts_position = cv
        .provenance
        .extra
        .get(rill_lander::revert::REVERTS_KEY)
        .and_then(|s| s.parse::<u64>().ok());
    Ok(pb::Landing {
        name: super::names::trunk_position(ctx, seq),
        position: seq.get(),
        version: super::names::version(ctx, &version),
        version_hash: hash_to_proto(&version),
        change_id: id_to_proto(&cv.change_id),
        landed_by: pb::Principal::from(landed_by).into(),
        landed_at: timestamp_to_proto(at),
        reverts_position,
    })
}

/// Map a queue status onto the wire enum.
pub fn land_status_proto(status: &LandStatus) -> pb::LandStatus {
    match status {
        LandStatus::Queued => pb::LandStatus::Queued,
        LandStatus::Verifying | LandStatus::Landing => pb::LandStatus::Rebasing,
        LandStatus::Landed { .. } => pb::LandStatus::Landed,
        LandStatus::Failed { .. } | LandStatus::Conflict { .. } => pb::LandStatus::Failed,
        LandStatus::Returned { .. } | LandStatus::Ejected { .. } => pb::LandStatus::Cancelled,
    }
}

/// The wire form of a land request as recorded, without its outcome.
pub fn land_request_proto(
    ctx: &RepoContext,
    record: &rill_types::LandRequestRecord,
) -> pb::LandRequest {
    let ticket = record.ticket.0.to_string();
    pb::LandRequest {
        name: super::names::land_request(ctx, &ticket),
        ticket,
        change: super::names::change(ctx, &record.request.change_id),
        change_id: id_to_proto(&record.request.change_id),
        status: land_status_proto(&record.status).into(),
        requested_at: timestamp_to_proto(record.request.requested_at),
        ..Default::default()
    }
}

/// A land request with its outcome or its place in the queue.
pub async fn land_request(ctx: &RepoContext, ticket: LandTicket) -> Result<pb::LandRequest> {
    let record = ctx
        .lander
        .queue()
        .record(ticket)
        .await?
        .ok_or_else(|| Error::not_found(format!("land request {}", ticket.0)))?;
    let mut out = land_request_proto(ctx, &record);
    match &record.status {
        LandStatus::Landed { seq } => {
            out.trunk_position = Some(seq.get());
            let hash = ctx.repo.line_at(&record.request.line, *seq).await?;
            out.landed_hash = hash.map(|h| hash_to_proto(&h));
        }
        LandStatus::Failed { reason }
        | LandStatus::Returned { reason }
        | LandStatus::Ejected { reason } => out.error = Some(reason.clone()),
        LandStatus::Conflict { paths } => {
            out.error = Some(format!(
                "rebase onto trunk conflicts at: {}",
                paths
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
        LandStatus::Queued => {
            if let Some((at, wait)) = ctx.lander.position(ticket).await? {
                out.queue_position = Some(at as u32);
                out.eta_ms = Some(wait.as_millis() as u64);
            }
        }
        LandStatus::Verifying | LandStatus::Landing => {}
    }
    Ok(out)
}

/// Clamp a client-supplied limit: zero means the default.
pub fn limit(requested: u32, default: usize, max: usize) -> usize {
    if requested == 0 {
        default
    } else {
        (requested as usize).min(max)
    }
}
