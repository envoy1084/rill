//! Human and JSON rendering of new versions and checkout updates.

use rill_daemon::proto as dpb;
use rill_sdk::VersionResult;
use rill_types::RepoPathBuf;
use serde_json::{json, Value};

use crate::output::{short, Out};

pub(super) fn print_version(
    out: &mut Out<'_>,
    verb: &str,
    r: &VersionResult,
) -> std::io::Result<()> {
    out.line(format!(
        "{verb} change {} version {} ({} file change{})",
        out.id(&r.version.change_id.to_string()),
        out.id(&short(&r.hash)),
        r.version.file_changes.len(),
        if r.version.file_changes.len() == 1 {
            ""
        } else {
            "s"
        },
    ))?;
    let conflicts = r.conflicted_paths();
    if !conflicts.is_empty() {
        out.line(out.bad(&format!(
            "conflicts in: {}",
            conflicts
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )))?;
    }
    Ok(())
}

pub(super) fn join_paths(paths: &[impl AsRef<str>]) -> String {
    paths
        .iter()
        .map(|p| p.as_ref())
        .collect::<Vec<_>>()
        .join(", ")
}

/// What an update did, as `--json` prints it.
pub(crate) fn update_json(r: &dpb::UpdateMountResponse) -> Value {
    json!({
        "updated": r.updated,
        "removed": r.removed,
        "merged": r.merged,
        "conflicts": r.conflicts,
    })
}

/// The conflicted paths of an update.
pub(crate) fn conflicts_of(r: &dpb::UpdateMountResponse) -> Vec<RepoPathBuf> {
    r.conflicts
        .iter()
        .filter_map(|p| RepoPathBuf::new(p).ok())
        .collect()
}

pub(super) fn print_report(
    out: &mut Out<'_>,
    report: &dpb::UpdateMountResponse,
) -> std::io::Result<()> {
    for p in &report.updated {
        out.line(format!("{} {p}", out.good("U")))?;
    }
    for p in &report.removed {
        out.line(format!("{} {p}", out.bad("D")))?;
    }
    for p in &report.merged {
        out.line(format!("{} {p}", out.warn("M")))?;
    }
    for p in &report.conflicts {
        out.line(format!("{} {p}", out.bad("C")))?;
    }
    Ok(())
}
