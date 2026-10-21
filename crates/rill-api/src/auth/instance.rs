//! Who administers the instance (token keys, revocation of others' tokens
//! and sessions) and each organization (its identity providers, users,
//! agents and trusts).

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

/// Allow `ctx` to administer `org`: an instance administrator, or a member
/// of `org` with its admin role presenting an unnarrowed token for it.
pub async fn require_org_admin(state: &AppState, ctx: &AuthContext, org: &str) -> Result<()> {
    if require_instance_admin(state, ctx).is_ok() {
        return Ok(());
    }
    let member = ctx.capability.is_none() && ctx.claims.as_ref().is_some_and(|c| c.org == org);
    if member {
        if let Some(user) = state.identity.user(org, &ctx.principal.name).await? {
            if user.principal.kind == ctx.principal.kind
                && user.is_active()
                && user.has_role(rill_types::identity::OrgRole::Admin)
            {
                return Ok(());
            }
        }
    }
    Err(Error::denied(format!(
        "{} does not administer org {org:?}",
        ctx.principal
    )))
}
