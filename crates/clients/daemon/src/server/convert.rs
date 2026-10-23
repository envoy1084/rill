//! Daemon values on the wire, and daemon errors as Connect errors.

use connectrpc::{ConnectError, ErrorCode};
use rill_sdk::pb::ErrorReason;

use crate::error::Error;
use crate::mount::{Mount, MountMode, MountRecord};
use crate::overlay::{ChangeKind, LocalChange};
use crate::proto as pb;

pub(crate) fn mode_to_proto(mode: MountMode) -> pb::MountMode {
    match mode {
        MountMode::Eager => pb::MountMode::Eager,
        MountMode::Lazy => pb::MountMode::Lazy,
        MountMode::Snapshot => pb::MountMode::Snapshot,
    }
}

pub(crate) fn mode_from_proto(mode: buffa::EnumValue<pb::MountMode>) -> MountMode {
    match mode.as_known() {
        Some(pb::MountMode::Lazy) => MountMode::Lazy,
        Some(pb::MountMode::Snapshot) => MountMode::Snapshot,
        _ => MountMode::Eager,
    }
}

pub(crate) fn record_to_proto(r: &MountRecord) -> pb::Mount {
    pb::Mount {
        name: r.name(),
        path: r.path.display().to_string(),
        endpoint: r.endpoint.clone(),
        repo: r.repo.clone(),
        mode: mode_to_proto(r.mode).into(),
        workspace: r.workspace.map(|w| w.to_string()).unwrap_or_default(),
        base: r.base.clone(),
        base_version: r.base_version.map(|v| v.to_hex()).unwrap_or_default(),
        working_change: r.working_change.map(|c| c.to_string()).unwrap_or_default(),
        sparse: r
            .sparse
            .include
            .iter()
            .map(|p| p.as_str().to_string())
            .collect(),
        ..Default::default()
    }
}

pub(crate) fn mount_to_proto(m: &Mount) -> pb::Mount {
    pb::Mount {
        pending: m.pending_count() as u64,
        sync_error: m.sync_error.lock().clone().unwrap_or_default(),
        ..record_to_proto(&m.record())
    }
}

pub(crate) fn change_to_proto(c: &LocalChange) -> pb::LocalChange {
    let kind = match c.kind {
        ChangeKind::Added => pb::ChangeKind::Added,
        ChangeKind::Modified => pb::ChangeKind::Modified,
        ChangeKind::Deleted => pb::ChangeKind::Deleted,
        ChangeKind::Conflict => pb::ChangeKind::Conflict,
    };
    pb::LocalChange {
        path: c.path.as_str().to_string(),
        kind: kind.into(),
    }
}

/// A daemon error as the Connect error a client turns back into a
/// `rill_sdk::Error`, server errors relayed with their details.
pub(crate) fn to_connect(err: Error) -> ConnectError {
    let (code, reason) = match &err {
        Error::Sdk(e) => return e.to_connect(),
        Error::NotFound(_) => (ErrorCode::NotFound, ErrorReason::NotFound),
        Error::Invalid(_) => (ErrorCode::InvalidArgument, ErrorReason::InvalidField),
        Error::Conflict(_) => (
            ErrorCode::FailedPrecondition,
            ErrorReason::ConflictsUnresolved,
        ),
        Error::Unsupported(_) => (ErrorCode::Unimplemented, ErrorReason::Unimplemented),
        Error::Unavailable(_) => (ErrorCode::Unavailable, ErrorReason::Unavailable),
        _ => (ErrorCode::Internal, ErrorReason::Unspecified),
    };
    let status = rill_sdk::Error::Status {
        code,
        message: err.to_string(),
        reason: (reason != ErrorReason::Unspecified).then_some(reason),
        missing: Vec::new(),
        retry_after: None,
    };
    status.to_connect()
}
