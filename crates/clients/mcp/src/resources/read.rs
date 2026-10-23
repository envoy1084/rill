//! Reading resources: a file's bytes, or the same JSON the diff and
//! review tools return.

use base64::prelude::{Engine, BASE64_STANDARD};
use rill_sdk::Root;
use rmcp::model::{ReadResourceResult, Resource, ResourceContents, ResourceTemplate};
use serde_json::{json, Value};

use super::uri::ResourceUri;
use crate::error::{ToolError, ToolResult};
use crate::tools::{self, ToolContext};

/// Resource reads return whole documents; this bounds the JSON ones.
const RESOURCE_TOKENS: u64 = 50_000;

/// The URI templates clients can fill in.
pub fn templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new("rill://{repo}/files/{root}/{+path}", "file")
            .with_description("A file's content at a root (`trunk`, `trunk@N`, a version hash, a ref); `/` in the repo or root is %2F")
            .with_mime_type("text/plain"),
        ResourceTemplate::new("rill://{repo}/changes/{change}/diff", "change diff")
            .with_description("A change's current version against its parent, as unified diffs")
            .with_mime_type("application/json"),
        ResourceTemplate::new("rill://{repo}/changes/{change}/review", "change review")
            .with_description("A change's review status: votes, open threads, policy decision, next step")
            .with_mime_type("application/json"),
    ]
}

/// The session's change as concrete resources, when it has one.
pub fn listed(ctx: &ToolContext) -> Vec<Resource> {
    let Ok(repo) = ctx.repo_name(&tools::Args::default()) else {
        return Vec::new();
    };
    let Some(change) = ctx.session().current(&repo) else {
        return Vec::new();
    };
    let diff = ResourceUri::Diff {
        repo: repo.clone(),
        change,
    };
    let review = ResourceUri::Review { repo, change };
    vec![
        Resource::new(diff.to_uri(), format!("{change} diff")).with_mime_type("application/json"),
        Resource::new(review.to_uri(), format!("{change} review"))
            .with_mime_type("application/json"),
    ]
}

/// Read `uri`.
pub async fn read(ctx: &ToolContext, uri: &str) -> ToolResult<ReadResourceResult> {
    let contents = match ResourceUri::parse(uri)? {
        ResourceUri::File { repo, root, path } => {
            let root: Root = root
                .parse()
                .map_err(|e| ToolError::invalid(format!("{uri}: {e}")))?;
            let data = ctx
                .client()
                .repo(repo)
                .read_file(&root, path.as_path())
                .await?;
            match String::from_utf8(data.to_vec()) {
                Ok(text) => ResourceContents::text(text, uri).with_mime_type("text/plain"),
                Err(_) => ResourceContents::blob(BASE64_STANDARD.encode(&data), uri)
                    .with_mime_type("application/octet-stream"),
            }
        }
        ResourceUri::Diff { repo, change } => {
            let args = json!({ "repo": repo, "change": change.to_string(), "mode": "diff", "max_tokens": RESOURCE_TOKENS });
            json_contents(uri, run(ctx, "rill_read", args).await?)
        }
        ResourceUri::Review { repo, change } => {
            let args = json!({ "repo": repo, "change": change.to_string(), "max_tokens": RESOURCE_TOKENS });
            json_contents(uri, run(ctx, "rill_review_status", args).await?)
        }
    };
    Ok(ReadResourceResult::new(vec![contents]))
}

async fn run(ctx: &ToolContext, tool: &str, args: Value) -> ToolResult<Value> {
    tools::call(ctx, tool, args)
        .await
        .unwrap_or_else(|| Err(ToolError::Other(format!("no tool {tool}"))))
}

fn json_contents(uri: &str, value: Value) -> ResourceContents {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    ResourceContents::text(text, uri).with_mime_type("application/json")
}
