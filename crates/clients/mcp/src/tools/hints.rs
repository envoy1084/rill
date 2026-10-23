//! The one next step a tool suggests, so an agent always knows how to
//! move its change forward without reading the docs.

use rill_sdk::SubmitDecision;
use rill_types::{ReviewState, ReviewStatus};

/// What to do next with a change in this state.
pub fn next_step(
    conflicts: usize,
    review: Option<&ReviewState>,
    decision: Option<&SubmitDecision>,
) -> String {
    if conflicts > 0 {
        return format!(
            "{conflicts} path(s) conflict: inspect them with rill_conflicts, then rill_resolve."
        );
    }
    let Some(review) = review else {
        return "Not submitted: check it with rill_check, then rill_commit to submit for review."
            .into();
    };
    match review.status {
        ReviewStatus::Landed => {
            return "Landed. Start the next change with rill_edit (no `change`).".into()
        }
        ReviewStatus::Abandoned => return "Abandoned. Start a new change with rill_edit.".into(),
        ReviewStatus::Landing => {
            return "Landing: call rill_land with `wait_seconds` to wait for the outcome.".into()
        }
        ReviewStatus::Open | ReviewStatus::Landable => {}
    }
    let open = review.unresolved_threads();
    if open > 0 {
        return format!(
            "{open} open thread(s): read them with rill_review_status, fix with rill_edit, answer with rill_respond."
        );
    }
    match decision {
        Some(d) if d.allowed => "Ready: land it with rill_land.".into(),
        Some(d) if !d.reasons.is_empty() => format!(
            "Waiting on review ({}); rill_watch waits for votes.",
            d.reasons.join("; ")
        ),
        _ => "Waiting on review; rill_watch waits for votes and comments.".into(),
    }
}
