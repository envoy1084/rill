//! Human rendering of review state.

use rill_types::{ReviewState, ReviewStatus};

use crate::output::{short, when, Out, Table};

pub(super) fn status_text(out: &Out<'_>, s: ReviewStatus) -> String {
    let name = format!("{s:?}").to_ascii_lowercase();
    match s {
        ReviewStatus::Landable | ReviewStatus::Landed => out.good(&name),
        ReviewStatus::Abandoned => out.bad(&name),
        _ => out.warn(&name),
    }
}

pub(super) fn print_state(out: &mut Out<'_>, r: &ReviewState) -> std::io::Result<()> {
    out.kv(&[
        ("change", out.id(&r.change_id.to_string())),
        ("status", status_text(out, r.status)),
        ("owner", r.owner.to_string()),
        ("risk", format!("{} {}", r.risk, r.risk_reasons.join("; "))),
        (
            "version",
            r.current_version().map(|h| short(&h)).unwrap_or_default(),
        ),
        ("versions", r.versions.len().to_string()),
        (
            "landed",
            r.landed_seq
                .map(|s| format!("trunk@{}", s.get()))
                .unwrap_or_default(),
        ),
        (
            "intent",
            r.intent.map(|i| i.to_string()).unwrap_or_default(),
        ),
        (
            "stack",
            r.stack
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            "threads",
            format!(
                "{} ({} unresolved)",
                r.threads.len(),
                r.unresolved_threads()
            ),
        ),
        ("updated", when(&r.updated_at)),
    ])?;
    let votes: Vec<_> = r.current_votes().collect();
    if !votes.is_empty() {
        out.blank()?;
        let mut table = Table::new(&["LABEL", "VALUE", "VOTER", "WHEN", "REASON"]);
        for v in votes {
            let value = if v.value > 0 {
                out.good(&format!("+{}", v.value))
            } else if v.value < 0 {
                out.bad(&v.value.to_string())
            } else {
                "0".to_string()
            };
            table.row(vec![
                v.label.clone(),
                value,
                v.voter.to_string(),
                when(&v.voted_at),
                v.reason.clone().unwrap_or_default(),
            ]);
        }
        out.table(table)?;
    }
    Ok(())
}
