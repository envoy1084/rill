//! Who administers the instance (token keys, revocation of others' tokens
//! and sessions) and each organization (its identity providers, users,
//! agents and trusts, and every repository in it).

use rill_policy::AuthContext;

use crate::error::{Error, Result};
use crate::identity::IdentityView;
use crate::state::AppState;

/// Allow `ctx` to administer the instance: anyone in dev mode, otherwise a
/// configured administrator presenting an unnarrowed token.
pub fn require_instance_admin(state: &AppState, ctx: &AuthContext) -> Result<()> {
    administers_instance(&state.identity, state.dev_mode(), ctx)
}

fn administers_instance(identity: &IdentityView, dev_mode: bool, ctx: &AuthContext) -> Result<()> {
    if dev_mode {
        return Ok(());
    }
    if ctx.capability.is_some() {
        return Err(Error::denied(
            "a narrowed token cannot administer the instance",
        ));
    }
    if identity.is_configured_admin(&ctx.principal) {
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
    if administers_org(&state.identity, state.dev_mode(), ctx, org).await? {
        return Ok(());
    }
    Err(Error::denied(format!(
        "{} does not administer org {org:?}",
        ctx.principal
    )))
}

/// Whether `ctx` administers `org`; see [`require_org_admin`].
pub async fn administers_org(
    identity: &IdentityView,
    dev_mode: bool,
    ctx: &AuthContext,
    org: &str,
) -> Result<bool> {
    if administers_instance(identity, dev_mode, ctx).is_ok() {
        return Ok(true);
    }
    let member = ctx.capability.is_none() && ctx.claims.as_ref().is_some_and(|c| c.org == org);
    if !member {
        return Ok(false);
    }
    Ok(identity
        .user(org, &ctx.principal.name)
        .await?
        .is_some_and(|user| {
            user.principal.kind == ctx.principal.kind
                && user.is_active()
                && user.has_role(rill_types::identity::OrgRole::Admin)
        }))
}
