//! Verifying JWTs from an OIDC issuer.

use std::sync::Arc;

use jsonwebtoken::{decode, decode_header, Algorithm, Validation};
use serde_json::{Map, Value};

use super::jwks::JwksCache;
use crate::error::{Error, Result};

/// Asymmetric algorithms accepted; symmetric ones and `none` never are.
const ALGORITHMS: [Algorithm; 8] = [
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::PS256,
    Algorithm::PS384,
    Algorithm::PS512,
    Algorithm::ES256,
    Algorithm::ES384,
];

/// Clock skew tolerated on `exp`, `nbf` and `iat`, in seconds.
const LEEWAY_SECS: u64 = 60;

/// A verified token's claims.
#[derive(Debug, Clone, PartialEq)]
pub struct Claims(pub Map<String, Value>);

impl Claims {
    pub fn str(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(Value::as_str)
    }

    pub fn subject(&self) -> Result<&str> {
        self.str("sub")
            .ok_or_else(|| Error::Oidc("token has no subject".into()))
    }

    pub fn issuer(&self) -> Result<&str> {
        self.str("iss")
            .ok_or_else(|| Error::Oidc("token has no issuer".into()))
    }

    /// A claim holding a list of strings (or one string).
    pub fn strings(&self, name: &str) -> Vec<String> {
        match self.0.get(name) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            Some(Value::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        }
    }
}

/// Verifies tokens from one issuer for a set of audiences.
pub struct JwtVerifier {
    issuer: String,
    audiences: Vec<String>,
    keys: Arc<JwksCache>,
}

impl JwtVerifier {
    pub fn new(issuer: impl Into<String>, audiences: Vec<String>, keys: Arc<JwksCache>) -> Self {
        JwtVerifier {
            issuer: issuer.into(),
            audiences,
            keys,
        }
    }

    /// Verify `token`: an accepted algorithm, a signature by one of the
    /// issuer's keys, the issuer, an audience, and the time claims. When
    /// `nonce` is given the token must carry it.
    pub async fn verify(&self, token: &str, nonce: Option<&str>) -> Result<Claims> {
        let header = decode_header(token).map_err(|e| Error::Oidc(format!("token: {e}")))?;
        if !ALGORITHMS.contains(&header.alg) {
            return Err(Error::Oidc(format!(
                "algorithm {:?} is not accepted",
                header.alg
            )));
        }
        let key = self.keys.key(header.kid.as_deref()).await?;
        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&self.audiences);
        validation.set_required_spec_claims(&["exp", "iss", "sub", "aud"]);
        validation.leeway = LEEWAY_SECS;
        validation.validate_nbf = true;
        let data = decode::<Map<String, Value>>(token, &key, &validation)
            .map_err(|e| Error::Oidc(format!("token: {e}")))?;
        let claims = Claims(data.claims);
        if let Some(expected) = nonce {
            if claims.str("nonce") != Some(expected) {
                return Err(Error::Oidc("token nonce does not match".into()));
            }
        }
        Ok(claims)
    }
}
