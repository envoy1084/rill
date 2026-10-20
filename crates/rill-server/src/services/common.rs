//! Helpers shared by every gRPC service: opening a request (caller plus
//! repository), resolving `RootSpec`s and building the wire records that
//! several services return.

use std::sync::Arc;

use rill_core::RootTree;
use rill_lander::{landing_of, LandStatus, LandTicket};
use rill_policy::AuthContext;
use rill_proto::convert::{
    hash_from_proto, hash_to_proto, id_to_proto, opt_hash_from_proto, timestamp_to_proto,
};
use rill_proto::rill::v1 as pb;
use rill_types::{
    ChangeVersionHash, OperationId, Principal, Provenance, RepoPathBuf, Timestamp, TreeHash,
    TrunkSeq,
};
use tonic::{Request, Response, Status};

use crate::auth::{caller, Permit};
use crate::error::{Error, Result};
use crate::state::{AppState, RepoContext};

/// What a handler returns.
pub type GrpcResult<T> = std::result::Result<Response<T>, Status>;

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
}

/// Resolve the caller and the repository named by `repo`.
pub async fn open<T>(state: &AppState, request: &Request<T>, repo: &str) -> Result<Opened> {
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

/// A point in history resolved to a root tree, with the version when the
/// spec named one.
#[derive(Debug, Clone, Copy)]
pub struct Resolved {
    /// The change version, if the spec named one (not for bare trees).
    pub version: Option<ChangeVersionHash>,
    /// The root tree.
    pub root: TreeHash,
}

/// Resolve a `RootSpec`; `None` means the trunk head. An empty repository
/// resolves to the empty tree with no version.
pub async fn resolve_root(ctx: &RepoContext, spec: Option<&pb::RootSpec>) -> Result<Resolved> {
    use pb::root_spec::Root;
    let version = match spec.and_then(|s| s.root.as_ref()) {
        None => ctx.repo.trunk_head().await?.map(|(_, h)| h),
        Some(Root::Ref(name)) => {
            let name = rill_types::RefName::new(name.as_str())?;
            Some(
                ctx.repo
                    .get_ref(&name)
                    .await?
                    .ok_or_else(|| Error::not_found(format!("ref {name:?}")))?,
            )
        }
        Some(Root::VersionHash(bytes)) => Some(hash_from_proto("version_hash", bytes)?),
        Some(Root::TrunkSeq(seq)) => Some(
            ctx.repo
                .trunk_at(TrunkSeq(*seq))
                .await?
                .ok_or_else(|| Error::not_found(format!("trunk@{seq}")))?,
        ),
        Some(Root::TreeHash(bytes)) => {
            let root = hash_from_proto("tree_hash", bytes)?;
            ctx.trees.get_tree(&root).await?;
            return Ok(Resolved {
                version: None,
                root,
            });
        }
    };
    let root = match version {
        Some(v) => ctx.derive.get::<RootTree>(&v).await?,
        None => ctx.trees.empty_tree().await?,
    };
    Ok(Resolved { version, root })
}

/// Resolve a `RootSpec` that must name a version (not a bare tree).
pub async fn resolve_version(
    ctx: &RepoContext,
    spec: Option<&pb::RootSpec>,
) -> Result<Option<ChangeVersionHash>> {
    let resolved = resolve_root(ctx, spec).await?;
    if matches!(
        spec.and_then(|s| s.root.as_ref()),
        Some(pb::root_spec::Root::TreeHash(_))
    ) {
        return Err(Error::invalid("a tree hash does not name a version"));
    }
    Ok(resolved.version)
}

/// The root tree of a version.
pub async fn root_of(ctx: &RepoContext, version: &ChangeVersionHash) -> Result<TreeHash> {
    Ok(ctx.derive.get::<RootTree>(version).await?)
}

/// Decode a required 32-byte hash field.
pub fn hash(field: &'static str, bytes: &[u8]) -> Result<ChangeVersionHash> {
    Ok(hash_from_proto(field, bytes)?)
}

/// Decode an optional hash field.
pub fn opt_hash(
    field: &'static str,
    bytes: Option<&bytes::Bytes>,
) -> Result<Option<ChangeVersionHash>> {
    Ok(opt_hash_from_proto(field, bytes)?)
}

/// Decode a path field.
pub fn path(field: &'static str, value: &str) -> Result<RepoPathBuf> {
    Ok(rill_proto::convert::path_from_proto(field, value)?)
}

/// The operation id a write request carries, or a fresh one.
pub fn operation_id(bytes: &[u8]) -> Result<OperationId> {
    if bytes.is_empty() {
        return Ok(OperationId::new());
    }
    Ok(rill_proto::convert::id_from_proto("operation_id", bytes)?)
}

/// Decode an optional provenance message.
pub fn provenance(value: Option<pb::Provenance>) -> Result<Provenance> {
    match value {
        Some(p) => Ok(Provenance::try_from(p)?),
        None => Ok(Provenance::default()),
    }
}

/// The wire record for a repository.
pub async fn repo_proto(ctx: &RepoContext) -> Result<pb::Repo> {
    let head = ctx.repo.trunk_head().await?;
    Ok(pb::Repo {
        id: id_to_proto(&ctx.config.id),
        name: ctx.config.name.clone(),
        description: (!ctx.config.description.is_empty()).then(|| ctx.config.description.clone()),
        created_at: timestamp_to_proto(ctx.config.created_at),
        trunk_head_seq: head.map(|(s, _)| s.get()).unwrap_or(0),
    })
}

/// The wire record for a change version's metadata.
pub async fn change_info(ctx: &RepoContext, hash: &ChangeVersionHash) -> Result<pb::ChangeInfo> {
    let info = ctx
        .repo
        .get_change_info(hash)
        .await?
        .ok_or_else(|| Error::not_found(format!("change version {hash}")))?;
    Ok(pb::ChangeInfo::from(info))
}

/// The wire record for a landing at `seq`.
pub async fn land_event(
    ctx: &RepoContext,
    seq: TrunkSeq,
    version: ChangeVersionHash,
) -> Result<pb::LandEvent> {
    let cv = ctx.derive.change(&version).await?;
    let landing = landing_of(&ctx.repo, &version).await?;
    let (landed_by, at) = match &landing {
        Some(l) => (l.requester.clone(), l.landed_at),
        None => (cv.author.clone(), cv.created_at),
    };
    let reverts_seq = cv
        .provenance
        .extra
        .get(rill_lander::revert::REVERTS_KEY)
        .and_then(|s| s.parse::<u64>().ok());
    Ok(pb::LandEvent {
        seq: seq.get(),
        hash: hash_to_proto(&version),
        change_id: id_to_proto(&cv.change_id),
        landed_by: Some(pb::Principal::from(landed_by)),
        at: timestamp_to_proto(at),
        reverts_seq,
    })
}

/// Map a queue status onto the wire enum.
pub fn land_status_proto(status: &LandStatus) -> pb::LandStatus {
    match status {
        LandStatus::Queued => pb::LandStatus::Queued,
        LandStatus::Verifying | LandStatus::Landing => pb::LandStatus::Rebasing,
        LandStatus::Landed { .. } => pb::LandStatus::Landed,
        LandStatus::Failed { .. } | LandStatus::Conflict { .. } => pb::LandStatus::Failed,
        LandStatus::Returned { .. } => pb::LandStatus::Cancelled,
    }
}

/// Look up a ticket's current status, falling back to the review state
/// once the queue has forgotten a terminal ticket.
pub async fn ticket_status(
    ctx: &RepoContext,
    ticket: LandTicket,
) -> Result<(pb::LandTicket, LandStatus)> {
    let info = ctx
        .tickets
        .get(&ticket.0)
        .map(|t| t.clone())
        .ok_or_else(|| Error::not_found(format!("land ticket {}", ticket.0)))?;
    let status = match ctx.lander.queue().status(ticket) {
        Some(status) => status,
        None => match ctx.repo.get_review(&info.change_id).await? {
            Some(state) if state.landed_seq.is_some() => LandStatus::Landed {
                seq: state.landed_seq.unwrap_or_default(),
            },
            _ => LandStatus::Returned {
                reason: "ticket expired before it reached a terminal state".into(),
            },
        },
    };
    let proto = pb::LandTicket {
        id: ticket.0.to_string(),
        change_id: id_to_proto(&info.change_id),
        status: land_status_proto(&status) as i32,
        requested_at: timestamp_to_proto(info.requested_at),
    };
    Ok((proto, status))
}

/// Parse a ticket id string.
pub fn parse_ticket(id: &str) -> Result<LandTicket> {
    id.trim_start_matches("land#")
        .parse::<u64>()
        .map(LandTicket)
        .map_err(|_| Error::invalid(format!("invalid land ticket {id:?}")))
}

/// Record an issued ticket so status lookups outlive the queue entry.
pub fn remember_ticket(ctx: &RepoContext, ticket: LandTicket, change_id: rill_types::ChangeId) {
    ctx.tickets.insert(
        ticket.0,
        crate::state::TicketInfo {
            change_id,
            requested_at: Timestamp::now(),
        },
    );
}

/// Clamp a client-supplied limit: zero means the default.
pub fn limit(requested: u32, default: usize, max: usize) -> usize {
    if requested == 0 {
        default
    } else {
        (requested as usize).min(max)
    }
}
