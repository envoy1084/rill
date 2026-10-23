//! What every tool call runs with: a client (chosen by the caller's
//! token), the default repository and the session.

use std::sync::Arc;

use rill_sdk::{Client, RepoClient};
use rill_types::ChangeId;

use super::args::Args;
use crate::error::{ToolError, ToolResult};
use crate::pool::ClientPool;
use crate::session::Session;

/// The connection, default repository and session of one call.
#[derive(Clone, Debug)]
pub struct ToolContext {
    pool: Arc<ClientPool>,
    client: Client,
    default_repo: Option<String>,
    session: Arc<Session>,
}

impl ToolContext {
    /// A context for a new session over `client`.
    pub fn new(client: Client, default_repo: Option<String>) -> Self {
        ToolContext {
            pool: Arc::new(ClientPool::new(client.clone())),
            client,
            default_repo,
            session: Arc::default(),
        }
    }

    /// The same connections with a new, empty session.
    pub fn new_session(&self) -> Self {
        ToolContext {
            client: self.pool.base().clone(),
            session: Arc::default(),
            ..self.clone()
        }
    }

    /// This context acting with `token` (an HTTP caller's bearer token)
    /// instead of the configured credentials.
    pub fn with_token(&self, token: Option<&str>) -> ToolResult<Self> {
        Ok(ToolContext {
            client: self.pool.for_token(token)?,
            ..self.clone()
        })
    }

    /// The connection.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// The session.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The repository a call names: its `repo` argument, else the default.
    pub fn repo_name(&self, args: &Args) -> ToolResult<String> {
        args.opt_str("repo")
            .map(str::to_string)
            .or_else(|| self.default_repo.clone())
            .ok_or(ToolError::NoRepo)
    }

    /// The repository a call addresses.
    pub fn repo(&self, args: &Args) -> ToolResult<RepoClient> {
        Ok(self.client.repo(self.repo_name(args)?))
    }

    /// The change a call names (`change`), else the session's.
    pub fn opt_change(&self, args: &Args) -> ToolResult<Option<ChangeId>> {
        match args.opt_parse::<ChangeId>("change")? {
            Some(id) => Ok(Some(id)),
            None => Ok(self.session.current(&self.repo_name(args)?)),
        }
    }

    /// Like [`ToolContext::opt_change`] but required.
    pub fn change(&self, args: &Args) -> ToolResult<ChangeId> {
        self.opt_change(args)?.ok_or(ToolError::NoChange)
    }

    /// Record `id` as the session's change in the call's repository.
    pub fn work_on(&self, args: &Args, id: ChangeId) -> ToolResult<()> {
        self.session.set_current(&self.repo_name(args)?, id);
        Ok(())
    }
}
