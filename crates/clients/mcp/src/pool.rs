//! One SDK client per bearer token. Over HTTP each request may carry its
//! own credentials; reusing a client per token keeps its connection and
//! session state instead of reconnecting on every call.

use std::collections::HashMap;
use std::sync::Mutex;

use rill_sdk::{Client, ClientConfig};

use crate::error::ToolResult;

/// Tokens past this many evict the whole cache rather than tracking
/// recency; a server sees few distinct callers.
const MAX_CLIENTS: usize = 256;

/// The configured client plus clients made for other bearer tokens.
#[derive(Debug)]
pub struct ClientPool {
    base: Client,
    by_token: Mutex<HashMap<String, Client>>,
}

impl ClientPool {
    /// A pool whose default is `base`.
    pub fn new(base: Client) -> Self {
        ClientPool {
            base,
            by_token: Mutex::new(HashMap::new()),
        }
    }

    /// The configured client.
    pub fn base(&self) -> &Client {
        &self.base
    }

    /// The client for `token`, or the configured one when there is none
    /// or it is the configured token.
    pub fn for_token(&self, token: Option<&str>) -> ToolResult<Client> {
        let Some(token) = token.filter(|t| !t.is_empty()) else {
            return Ok(self.base.clone());
        };
        if self.base.config().token.as_deref() == Some(token) {
            return Ok(self.base.clone());
        }
        // The lock is never held across an await, so a poisoned map only
        // means a panic mid-insert; its contents are still valid clients.
        let mut map = self.by_token.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(client) = map.get(token) {
            return Ok(client.clone());
        }
        let config = ClientConfig {
            token: Some(token.to_string()),
            session: None,
            workload: None,
            ..self.base.config().clone()
        };
        let client = Client::connect_lazy(config)?;
        if map.len() >= MAX_CLIENTS {
            map.clear();
        }
        map.insert(token.to_string(), client.clone());
        Ok(client)
    }
}
