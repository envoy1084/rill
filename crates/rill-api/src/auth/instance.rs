//! Who administers the instance: token keys, revocation of others'
//! tokens and sessions.

use rill_policy::AuthContext;

use crate::error::{Error, Result};
use crate::state::AppState;

/// Allow `ctx` to administer the instance: anyone in dev mode, otherwise a
/// configured administrator presenting an unnarrowed token.
pub fn require_instance_admin(state: &AppState, ctx: &AuthContext) -> Result<()> {
    if state.dev_mode() {
        return Ok(());
    }
    if ctx.capability.is_some() {
        return Err(Error::denied(
            "a narrowed token cannot administer the instance",
        ));
    }
    if state.identity.is_configured_admin(&ctx.principal) {
        return Ok(());
    }
    Err(Error::denied(format!(
        "{} does not administer this instance",
        ctx.principal
    )))
}
