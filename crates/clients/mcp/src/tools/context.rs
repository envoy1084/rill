//! The connection and default repository every tool handler runs with.

use rill_sdk::{Client, RepoClient};

use super::args::Args;
use crate::error::{ToolError, ToolResult};

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
