//! Minting a token outside a running server, for bootstrap: whoever can
//! reach the metadata store and the key service can sign as the instance.

use rill_policy::TokenClaims;
use rill_types::{Principal, Timestamp};

use super::view::IdentityView;
use crate::config::ServerConfig;
use crate::error::{Error, Result};
use crate::state::open_stores;

/// Sign an unnarrowed token for `principal` in `org`, living `ttl_secs`
/// (the longest its kind allows when `None`).
pub async fn mint_token(
    config: &ServerConfig,
    principal: &Principal,
    org: &str,
    ttl_secs: Option<u64>,
) -> Result<String> {
    let stores = open_stores(&config.storage, &config.metadata).await?;
    let view = IdentityView::open(&stores, &config.identity).await?;
    let issuer = view.issuer()?;
    let longest = issuer.lifetimes().max_for(principal.kind);
    let ttl = match ttl_secs {
        Some(secs) => (secs as i64).saturating_mul(1000),
        None => longest,
    };
    if ttl > longest {
        return Err(Error::invalid(format!(
            "a {} token may live at most {} s",
            principal.kind.as_str(),
            longest / 1000
        )));
    }
    let claims = TokenClaims::new(org, Timestamp::now(), ttl);
    let token = issuer.issue(principal, &claims, None)?;
    tracing::info!(principal = %principal, token = %claims.token_id, "token minted");
    Ok(token)
}
