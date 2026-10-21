//! Sessions a sign-in starts, their refresh tokens, and the access tokens
//! issued under them.

use std::fmt;
use std::str::FromStr;

use base64::Engine as _;
use rill_policy::{Issuer, TokenClaims};
use rill_store::identity::IdentityStore;
use rill_types::identity::{SessionKind, SessionRecord, UserRecord};
use rill_types::{SessionId, Timestamp};

use crate::error::{Error, Result};

/// A refresh token: the session id and a secret only its holder knows.
/// The store keeps a hash of the secret, and each use replaces it.
#[derive(Clone, PartialEq, Eq)]
pub struct RefreshToken {
    pub session: SessionId,
    secret: [u8; 32],
}

impl RefreshToken {
    fn new(session: SessionId) -> Self {
        RefreshToken {
            session,
            secret: rand::random(),
        }
    }

    fn hash(&self) -> [u8; 32] {
        *blake3::keyed_hash(
            &blake3::derive_key("rill session refresh secret", self.session.as_bytes()),
            &self.secret,
        )
        .as_bytes()
    }
}

impl fmt::Display for RefreshToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(self.secret);
        write!(f, "{}.{secret}", self.session)
    }
}

impl fmt::Debug for RefreshToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RefreshToken({})", self.session)
    }
}

impl FromStr for RefreshToken {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let bad = || Error::Denied("malformed refresh token".into());
        let (session, secret) = s.split_once('.').ok_or_else(bad)?;
        let session: SessionId = session.parse().map_err(|_| bad())?;
        let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(secret)
            .map_err(|_| bad())?;
        Ok(RefreshToken {
            session,
            secret: secret.try_into().map_err(|_| bad())?,
        })
    }
}

/// What a sign-in or refresh hands the client.
#[derive(Debug, Clone)]
pub struct SessionTokens {
    pub access_token: String,
    pub expires_at: Timestamp,
    pub refresh_token: RefreshToken,
    pub session: SessionRecord,
}

/// An access token for `session`, living as long as its kind allows but
/// never past the session.
pub fn access_token(
    issuer: &Issuer,
    session: &SessionRecord,
    now: Timestamp,
) -> Result<(String, Timestamp)> {
    let longest = issuer.lifetimes().max_for(session.principal.kind);
    let ends = (now.millis() + longest).min(session.expires_at.millis());
    let mut claims =
        TokenClaims::new(&session.org, now, ends - now.millis()).with_session(session.id);
    claims.sponsor = session.sponsor.clone();
    claims.model = session.model.clone();
    let token = issuer.issue(&session.principal, &claims, None)?;
    Ok((token, claims.expires_at))
}

/// Start a session for `user` lasting `ttl_ms`, with a refresh token and
/// a first access token.
pub async fn open_session(
    store: &IdentityStore,
    issuer: &Issuer,
    user: &UserRecord,
    kind: SessionKind,
    ttl_ms: i64,
    now: Timestamp,
) -> Result<SessionTokens> {
    let id = SessionId::new();
    let refresh_token = RefreshToken::new(id);
    let session = SessionRecord {
        id,
        org: user.org.clone(),
        principal: user.principal.clone(),
        kind,
        created_at: now,
        expires_at: Timestamp::from_millis(now.millis() + ttl_ms),
        refresh_hash: Some(refresh_token.hash()),
        revoked_at: None,
        sponsor: None,
        model: None,
    };
    store.create_session(&session).await?;
    let (access_token, expires_at) = access_token(issuer, &session, now)?;
    Ok(SessionTokens {
        access_token,
        expires_at,
        refresh_token,
        session,
    })
}

/// Use a refresh token: a new access token and a new refresh token, the
/// old one spent. A spent, unknown or revoked token is refused.
pub async fn refresh_session(
    store: &IdentityStore,
    issuer: &Issuer,
    token: &RefreshToken,
    now: Timestamp,
) -> Result<SessionTokens> {
    let refused = || Error::Denied("the refresh token is not valid".into());
    let next = RefreshToken::new(token.session);
    let session = store
        .rotate_refresh(&token.session, token.hash(), next.hash())
        .await?
        .ok_or_else(refused)?;
    if !session.is_live_at(now) {
        return Err(refused());
    }
    let (access_token, expires_at) = access_token(issuer, &session, now)?;
    Ok(SessionTokens {
        access_token,
        expires_at,
        refresh_token: next,
        session,
    })
}
