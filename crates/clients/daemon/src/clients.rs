//! One server client per endpoint, rebuilt when a caller brings a new
//! token. Tokens live in memory only.

use std::collections::HashMap;

use parking_lot::Mutex;
use rill_sdk::Client;

use crate::config::ConfigSource;
use crate::error::Result;

/// Clients by endpoint.
pub struct Clients {
    source: ConfigSource,
    entries: Mutex<HashMap<String, (Option<String>, Client)>>,
}

impl Clients {
    /// Clients configured by `source`.
    pub fn new(source: ConfigSource) -> Self {
        Clients {
            source,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Remember the token a caller passed for `endpoint`; an empty token
    /// leaves the current one.
    pub fn offer_token(&self, endpoint: &str, token: &str) {
        if token.is_empty() {
            return;
        }
        let mut entries = self.entries.lock();
        if entries
            .get(endpoint)
            .is_some_and(|(t, _)| t.as_deref() == Some(token))
        {
            return;
        }
        entries.remove(endpoint);
        drop(entries);
        if let Err(e) = self.build(endpoint, Some(token.to_string())) {
            tracing::warn!(endpoint, error = %e, "client for the new token not built");
        }
    }

    /// The client for `endpoint`.
    pub fn get(&self, endpoint: &str) -> Result<Client> {
        if let Some((_, client)) = self.entries.lock().get(endpoint) {
            return Ok(client.clone());
        }
        self.build(endpoint, None)
    }

    fn build(&self, endpoint: &str, token: Option<String>) -> Result<Client> {
        let client = Client::connect_lazy((self.source)(endpoint, token.clone()))?;
        self.entries
            .lock()
            .insert(endpoint.to_string(), (token, client.clone()));
        Ok(client)
    }
}
