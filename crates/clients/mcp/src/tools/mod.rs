//! The tool registry: definitions (name, description, JSON schema) and
//! dispatch of `tools/call` into the SDK.

mod admin;
mod args;
mod intent;
mod lander;
mod repo;
mod review;
mod schema;
mod workspace;
mod write;

use rill_sdk::{Client, RepoClient};
use serde_json::Value;

pub use args::Args;

use crate::error::{ToolError, ToolResult};

/// A tool as advertised by `tools/list`.
#[derive(Debug, Clone)]
pub struct ToolDef {
    /// Tool name, `rill_*`.
    pub name: &'static str,
    /// What the tool does, for the model.
    pub description: &'static str,
    /// JSON schema of the `arguments` object.
    pub input_schema: Value,
}

impl ToolDef {
    /// The `tools/list` entry.
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        })
    }
}

/// What every tool handler needs: a connection and the default repository.
#[derive(Clone, Debug)]
pub struct ToolContext {
    client: Client,
    default_repo: Option<String>,
}

impl ToolContext {
    /// Build a context.
    pub fn new(client: Client, default_repo: Option<String>) -> Self {
        ToolContext {
            client,
            default_repo,
        }
    }

    /// The connection.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// The repository a call addresses: its `repo` argument, else the
    /// configured default.
    pub fn repo(&self, args: &Args) -> ToolResult<RepoClient> {
        let name = args
            .opt_str("repo")
            .map(str::to_string)
            .or_else(|| self.default_repo.clone())
            .ok_or(ToolError::NoRepo)?;
        Ok(self.client.repo(name))
    }
}

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
