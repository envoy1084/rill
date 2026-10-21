//! What every token says about itself, whatever it grants.

use rill_types::{Principal, PrincipalKind, SessionId, Timestamp, TokenId};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Claims carried by a token's authority block. The server trusts these
/// (they are signed) and stamps them on the writes the token makes; a
/// client never supplies them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenClaims {
    /// Revocation id of this token and every token attenuated from it.
    pub token_id: TokenId,
    /// Organization the principal belongs to.
    pub org: String,
    /// The sign-in session or agent run the token was issued for.
    pub session: Option<SessionId>,
    /// The human or team an agent acts for.
    pub sponsor: Option<String>,
    /// The model an agent declared at exchange, checked against its
    /// registration.
    pub model: Option<String>,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
}

impl TokenClaims {
    /// Claims for a new token in `org` living `ttl_ms` from `now`.
    pub fn new(org: impl Into<String>, now: Timestamp, ttl_ms: i64) -> Self {
        TokenClaims {
            token_id: TokenId::new(),
            org: org.into(),
            session: None,
            sponsor: None,
            model: None,
            issued_at: now,
            expires_at: Timestamp::from_millis(now.millis() + ttl_ms),
        }
    }

    pub fn with_session(mut self, session: SessionId) -> Self {
        self.session = Some(session);
        self
    }

    pub fn with_sponsor(mut self, sponsor: impl Into<String>) -> Self {
        self.sponsor = Some(sponsor.into());
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// How long the token lives, in milliseconds.
    pub fn lifetime_ms(&self) -> i64 {
        self.expires_at.millis() - self.issued_at.millis()
    }
}

/// The longest a token may live, per principal kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenLifetimes {
    pub human_ms: i64,
    pub agent_ms: i64,
    pub service_ms: i64,
}

impl Default for TokenLifetimes {
    fn default() -> Self {
        TokenLifetimes {
            human_ms: 3_600_000,
            agent_ms: 900_000,
            service_ms: 12 * 3_600_000,
        }
    }
}

impl TokenLifetimes {
    /// The maximum lifetime for `kind`.
    pub fn max_for(&self, kind: PrincipalKind) -> i64 {
        match kind {
            PrincipalKind::Human => self.human_ms,
            PrincipalKind::Agent => self.agent_ms,
            PrincipalKind::Service => self.service_ms,
        }
    }

    /// Refuse claims that live longer than `principal`'s kind allows or
    /// that end before they start.
    pub fn check(&self, principal: &Principal, claims: &TokenClaims) -> Result<()> {
        let max = self.max_for(principal.kind);
        let lifetime = claims.lifetime_ms();
        if lifetime <= 0 {
            return Err(Error::Token("token expires before it is issued".into()));
        }
        if lifetime > max {
            return Err(Error::Token(format!(
                "a {} token may live at most {} s, not {} s",
                principal.kind.as_str(),
                max / 1000,
                lifetime / 1000
            )));
        }
        Ok(())
    }
}
