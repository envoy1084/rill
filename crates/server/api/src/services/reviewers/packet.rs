//! A review context on the wire.

use rill_proto::rill::v1 as pb;
use rill_review::ReviewContext;
use rill_types::RiskClass;

/// The packet a reviewer gets for `ctx`.
pub(super) fn packet(ctx: &ReviewContext, risk: RiskClass) -> pb::ReviewPacket {
    let files = ctx
        .diffs
        .iter()
        .map(|d| {
            // Added and deleted files have no hunks in the context; show
            // their whole text.
            let hunks = match (&d.before, &d.after) {
                (None, Some(after)) if d.hunks.is_empty() => rill_core::diff_lines(b"", after),
                (Some(before), None) if d.hunks.is_empty() => rill_core::diff_lines(before, b""),
                _ => d.hunks.clone(),
            };
            let unified = rill_core::to_unified(d.path.as_str(), &hunks);
            pb::ReviewFile {
                path: d.path.to_string(),
                unified: String::from_utf8_lossy(&unified).into_owned(),
                added: d.before.is_none() && d.after.is_some(),
                deleted: d.before.is_some() && d.after.is_none(),
            }
        })
        .collect();
    pb::ReviewPacket {
        description: ctx.change.message.clone(),
        author: pb::Principal::from(&ctx.change.author).into(),
        risk: risk.as_str().into(),
        intent_title: ctx
            .intent
            .as_ref()
            .map(|i| i.title.clone())
            .unwrap_or_default(),
        intent_body: ctx
            .intent
            .as_ref()
            .map(|i| i.body.clone())
            .unwrap_or_default(),
        files,
        truncated: ctx.truncated,
    }
}
