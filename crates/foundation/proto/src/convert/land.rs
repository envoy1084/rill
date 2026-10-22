//! Land requests: lanes, tickets and statuses.

use rill_types::{LandStatus, LandTicket, Lane, RepoPathBuf, TrunkSeq};

use crate::error::{ConvertError, Result};

/// A lane by name.
pub fn lane_from_proto(field: &'static str, name: &str) -> Result<Lane> {
    Lane::ALL
        .into_iter()
        .find(|lane| lane.name() == name)
        .ok_or_else(|| ConvertError::Unknown {
            field,
            value: name.to_string(),
        })
}

/// Tickets travel as decimal strings.
pub fn ticket_from_proto(field: &'static str, id: &str) -> Result<LandTicket> {
    id.trim_start_matches("land#")
        .parse::<u64>()
        .map(LandTicket)
        .map_err(|_| ConvertError::Unknown {
            field,
            value: id.to_string(),
        })
}

/// A status as its name, reason, landed position and conflict paths.
pub fn land_status_parts(
    status: &LandStatus,
) -> (String, Option<String>, Option<u64>, Vec<String>) {
    let name = status.name().to_string();
    match status {
        LandStatus::Landed { seq } => (name, None, Some(seq.get()), Vec::new()),
        LandStatus::Failed { reason }
        | LandStatus::Returned { reason }
        | LandStatus::Ejected { reason } => (name, Some(reason.clone()), None, Vec::new()),
        LandStatus::Conflict { paths } => (
            name,
            None,
            None,
            paths.iter().map(ToString::to_string).collect(),
        ),
        LandStatus::Queued | LandStatus::Verifying | LandStatus::Landing => {
            (name, None, None, Vec::new())
        }
    }
}

/// The status [`land_status_parts`] took apart.
pub fn land_status_from_parts(
    field: &'static str,
    name: &str,
    reason: Option<String>,
    seq: Option<u64>,
    paths: &[String],
) -> Result<LandStatus> {
    let reason = || reason.clone().unwrap_or_default();
    Ok(match name {
        "queued" => LandStatus::Queued,
        "verifying" => LandStatus::Verifying,
        "landing" => LandStatus::Landing,
        "landed" => LandStatus::Landed {
            seq: TrunkSeq(seq.ok_or(ConvertError::Missing { field })?),
        },
        "failed" => LandStatus::Failed { reason: reason() },
        "returned" => LandStatus::Returned { reason: reason() },
        "ejected" => LandStatus::Ejected { reason: reason() },
        "conflict" => LandStatus::Conflict {
            paths: paths
                .iter()
                .map(|p| {
                    RepoPathBuf::new(p).map_err(|source| ConvertError::Invalid { field, source })
                })
                .collect::<Result<_>>()?,
        },
        other => {
            return Err(ConvertError::Unknown {
                field,
                value: other.to_string(),
            })
        }
    })
}
