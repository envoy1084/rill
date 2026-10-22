//! Discovery documents and key sets per issuer, shared by every request.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use super::discovery::{discover, ProviderMetadata};
use super::jwks::{JwksCache, JwksSource};
use super::jwt::JwtVerifier;
use crate::error::Result;

struct Issuer {
    meta: ProviderMetadata,
    keys: Arc<JwksCache>,
}

/// What this process knows about OIDC issuers.
#[derive(Clone)]
pub struct Issuers {
    client: reqwest::Client,
    issuers: Arc<Mutex<HashMap<String, Arc<Issuer>>>>,
}

impl Issuers {
    pub fn new(client: reqwest::Client) -> Self {
        Issuers {
            client,
            issuers: Arc::default(),
        }
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn issuer(&self, issuer: &str) -> Result<Arc<Issuer>> {
        if let Some(found) = self.issuers.lock().get(issuer) {
            return Ok(found.clone());
        }
        let meta = discover(&self.client, issuer).await?;
        let keys = Arc::new(JwksCache::new(
            self.client.clone(),
            JwksSource::Uri(meta.jwks_uri.clone()),
        ));
        let found = Arc::new(Issuer { meta, keys });
        self.issuers
            .lock()
            .insert(issuer.to_string(), found.clone());
        Ok(found)
    }

    /// The issuer's discovery document.
    pub async fn metadata(&self, issuer: &str) -> Result<ProviderMetadata> {
        Ok(self.issuer(issuer).await?.meta.clone())
    }

    /// A verifier for tokens `issuer` signs for `audiences`, with keys
    /// from its discovery document.
    pub async fn verifier(&self, issuer: &str, audiences: Vec<String>) -> Result<JwtVerifier> {
        let found = self.issuer(issuer).await?;
        Ok(JwtVerifier::new(issuer, audiences, found.keys.clone()))
    }
}
