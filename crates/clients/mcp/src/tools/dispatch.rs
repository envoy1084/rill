//! The full tool list and routing of `tools/call` to each tool group.

use serde_json::Value;

use super::args::Args;
use super::{admin, intent, lander, repo, review, workspace, write};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;

/// Every tool, in a stable order.
pub fn definitions() -> Vec<ToolDef> {
    let mut all = Vec::new();
    all.extend(repo::definitions());
    all.extend(write::definitions());
    all.extend(review::definitions());
    all.extend(lander::definitions());
    all.extend(intent::definitions());
    all.extend(workspace::definitions());
    all.extend(admin::definitions());
    all
}

/// Run a tool. `None` means no tool has that name.
pub async fn call(ctx: &ToolContext, name: &str, arguments: Value) -> Option<ToolResult<Value>> {
    let args = match Args::new(arguments) {
        Ok(a) => a,
        Err(e) => {
            return definitions()
                .iter()
                .any(|d| d.name == name)
                .then_some(Err(e))
        }
    };
    if let Some(r) = repo::call(ctx, name, &args).await {
        return Some(r);
    }
    if let Some(r) = write::call(ctx, name, &args).await {
        return Some(r);
    }
    if let Some(r) = review::call(ctx, name, &args).await {
        return Some(r);
    }
    if let Some(r) = lander::call(ctx, name, &args).await {
        return Some(r);
    }
    if let Some(r) = intent::call(ctx, name, &args).await {
        return Some(r);
    }
    if let Some(r) = workspace::call(ctx, name, &args).await {
        return Some(r);
    }
    admin::call(ctx, name, &args).await
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
