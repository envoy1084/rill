//! JSON Web Key Sets, fetched and cached per issuer.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::DecodingKey;
use parking_lot::RwLock;

use crate::error::{Error, Result};

/// How long fetched keys are trusted before refetching.
const TTL: Duration = Duration::from_secs(3600);
/// The fewest seconds between fetches forced by an unknown key id.
const MIN_REFETCH: Duration = Duration::from_secs(10);

/// Where an issuer's keys come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JwksSource {
    Uri(String),
    /// A JWKS document given inline, for issuers whose keys are not
    /// reachable (a cluster's service account issuer).
    Inline(String),
}

struct Cached {
    keys: HashMap<String, DecodingKey>,
    /// The only key when the set has one without a `kid`.
    anonymous: Option<DecodingKey>,
    fetched: Instant,
}

/// The keys of one issuer.
pub struct JwksCache {
    client: reqwest::Client,
    source: JwksSource,
    cached: RwLock<Option<Cached>>,
}

impl JwksCache {
    pub fn new(client: reqwest::Client, source: JwksSource) -> Self {
        JwksCache {
            client,
            source,
            cached: RwLock::new(None),
        }
    }

    /// The key `kid` names, fetching the set when it is stale or does not
    /// know the id.
    pub async fn key(&self, kid: Option<&str>) -> Result<DecodingKey> {
        if let Some(key) = self.lookup(kid, false) {
            return Ok(key);
        }
        let may_fetch = self
            .cached
            .read()
            .as_ref()
            .is_none_or(|c| c.fetched.elapsed() >= MIN_REFETCH);
        if may_fetch {
            let fresh = self.fetch().await?;
            *self.cached.write() = Some(fresh);
        }
        self.lookup(kid, true)
            .ok_or_else(|| Error::Oidc(format!("no signing key {kid:?}")))
    }

    fn lookup(&self, kid: Option<&str>, stale_ok: bool) -> Option<DecodingKey> {
        let cached = self.cached.read();
        let cached = cached.as_ref()?;
        if !stale_ok && cached.fetched.elapsed() >= TTL {
            return None;
        }
        match kid {
            Some(kid) => cached.keys.get(kid).cloned(),
            None => cached.anonymous.clone(),
        }
    }

    async fn fetch(&self) -> Result<Cached> {
        let text = match &self.source {
            JwksSource::Inline(text) => text.clone(),
            JwksSource::Uri(uri) => self
                .client
                .get(uri)
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| Error::Oidc(format!("keys at {uri}: {e}")))?
                .text()
                .await
                .map_err(|e| Error::Oidc(format!("keys at {uri}: {e}")))?,
        };
        let set: JwkSet =
            serde_json::from_str(&text).map_err(|e| Error::Oidc(format!("key set: {e}")))?;
        let mut keys = HashMap::new();
        let mut anonymous = None;
        for jwk in &set.keys {
            let Ok(key) = DecodingKey::from_jwk(jwk) else {
                continue;
            };
            match &jwk.common.key_id {
                Some(kid) => {
                    keys.insert(kid.clone(), key);
                }
                None if set.keys.len() == 1 => anonymous = Some(key),
                None => {}
            }
        }
        Ok(Cached {
            keys,
            anonymous,
            fetched: Instant::now(),
        })
    }
}
