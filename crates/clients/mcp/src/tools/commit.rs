//! `rill_commit`: finish a change. Sets its message, links its intent,
//! and submits it for review.

use rill_sdk::{ApplyOptions, EditBase};
use rill_types::IntentId;
use serde_json::{json, Value};

use super::args::Args;
use super::hints::next_step;
use super::provenance::provenance;
use super::schema::{boolean, provenance_props, string, Props};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;
use crate::json;

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_commit",
        description: "Finish the change: set its final message (first line is the summary), link it to an intent, and submit it for review (default). Returns the review status, what policy still needs, and the next step.",
        input_schema: provenance_props(Props::repo()
            .change()
            .opt("message", string("Final change message"))
            .opt("intent", string("Intent id to link the change to"))
            .opt("submit", boolean("Submit for review (default true)"))
            )
            .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let id = ctx.change(args)?;
    let mut info = repo.current_version(id).await?;
    if let Some(message) = args.opt_string("message")? {
        if message != info.message {
            let opts = ApplyOptions::message(message).with_provenance(provenance(args)?);
            let result = repo
                .apply_edits(EditBase::Change(id), Vec::new(), opts)
                .await?;
            info = repo.change_info(result.hash).await?;
        }
    }
    if let Some(intent) = args.opt_parse::<IntentId>("intent")? {
        repo.intents().link_change(intent, id).await?;
    }
    let mut out = json!({
        "change": id.to_string(),
        "version": info.hash.to_hex(),
        "message": info.summary(),
    });
    if !args.bool_or("submit", true)? {
        out["next"] = json!(next_step(info.has_conflicts as usize, None, None));
        return Ok(out);
    }
    let reviews = repo.reviews();
    // Committing again after edits is normal; the review already exists.
    let review = match reviews.submit(id).await {
        Ok(review) => review,
        Err(e) if e.code() == Some(rill_sdk::Code::AlreadyExists) => reviews.get(id).await?.review,
        Err(e) => return Err(e.into()),
    };
    let decision = reviews.evaluate_submit(id).await?;
    out["status"] = json::enum_name(&review.status);
    out["decision"] = json::submit_decision(&decision);
    out["next"] = json!(next_step(
        info.has_conflicts as usize,
        Some(&review),
        Some(&decision)
    ));
    Ok(out)
}
