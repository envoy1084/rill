//! The tool list and routing of `tools/call`.

use serde_json::Value;

use super::args::Args;
use super::{
    check, commit, conflicts, edit, land, overview, read, resolve, respond, search, status,
    symbols, sync, watch,
};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;

/// Every tool, in the order an agent meets them.
pub fn definitions() -> Vec<ToolDef> {
    vec![
        overview::definition(),
        read::definition(),
        search::definition(),
        symbols::definition(),
        edit::definition(),
        check::definition(),
        commit::definition(),
        status::definition(),
        respond::definition(),
        land::definition(),
        sync::definition(),
        conflicts::definition(),
        resolve::definition(),
        watch::definition(),
    ]
}

/// Run a tool. `None` means no tool has that name.
pub async fn call(ctx: &ToolContext, name: &str, arguments: Value) -> Option<ToolResult<Value>> {
    if !definitions().iter().any(|d| d.name == name) {
        return None;
    }
    let args = match Args::new(arguments) {
        Ok(a) => a,
        Err(e) => return Some(Err(e)),
    };
    Some(match name {
        "rill_context" => overview::call(ctx, &args).await,
        "rill_read" => read::call(ctx, &args).await,
        "rill_search" => search::call(ctx, &args).await,
        "rill_symbols" => symbols::call(ctx, &args).await,
        "rill_edit" => edit::call(ctx, &args).await,
        "rill_check" => check::call(ctx, &args).await,
        "rill_commit" => commit::call(ctx, &args).await,
        "rill_review_status" => status::call(ctx, &args).await,
        "rill_respond" => respond::call(ctx, &args).await,
        "rill_land" => land::call(ctx, &args).await,
        "rill_sync" => sync::call(ctx, &args).await,
        "rill_conflicts" => conflicts::call(ctx, &args).await,
        "rill_resolve" => resolve::call(ctx, &args).await,
        "rill_watch" => watch::call(ctx, &args).await,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_are_unique_and_well_formed() {
        let defs = definitions();
        let mut names: Vec<&str> = defs.iter().map(|d| d.name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate tool names");
        for d in &defs {
            assert!(d.name.starts_with("rill_"), "{}", d.name);
            assert!(!d.description.is_empty(), "{}", d.name);
            assert_eq!(d.input_schema["type"], "object", "{}", d.name);
            assert!(d.input_schema["properties"].is_object(), "{}", d.name);
        }
    }
}
