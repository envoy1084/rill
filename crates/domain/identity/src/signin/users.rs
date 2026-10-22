//! From a verified ID token to the user it signs in as.

use rill_store::identity::IdentityStore;
use rill_types::identity::{AccountStatus, ExternalIdentity, IdentityProvider, UserRecord};
use rill_types::{Principal, Timestamp};

use crate::error::{Error, Result};
use crate::oidc::Claims;

/// The principal name a provider's claims give, made safe for a name.
pub fn username_from(provider: &IdentityProvider, claims: &Claims) -> Result<String> {
    let raw = claims
        .str(&provider.username_claim)
        .or_else(|| claims.str("email"))
        .ok_or_else(|| {
            Error::Denied(format!(
                "the ID token has no {:?} or email claim",
                provider.username_claim
            ))
        })?;
    let name = raw.trim();
    let valid = !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|c| !c.is_control() && !c.is_whitespace());
    if !valid {
        return Err(Error::Denied(format!("{raw:?} cannot name a user")));
    }
    Ok(name.to_string())
}

/// The user `claims` sign in as through `provider`: the one its external
/// identity is linked to, or a new one when the provider provisions on
/// first sign-in. Teams follow the provider's groups claim on every
/// sign-in. Suspended users are refused.
pub async fn resolve_user(
    store: &IdentityStore,
    provider: &IdentityProvider,
    claims: &Claims,
    now: Timestamp,
) -> Result<UserRecord> {
    let external = ExternalIdentity {
        issuer: provider.issuer.clone(),
        subject: claims.subject()?.to_string(),
    };
    let actor = Principal::service("signin");
    let groups = provider
        .groups_claim
        .as_deref()
        .map(|claim| claims.strings(claim));
    let user = match store.user_by_external(&external).await? {
        Some(user) => user,
        None => {
            if !provider.auto_provision {
                return Err(Error::Denied(
                    "this identity is not provisioned in the organization".into(),
                ));
            }
            let name = username_from(provider, claims)?;
            let user = UserRecord {
                org: provider.org.clone(),
                principal: Principal::human(&name),
                email: claims.str("email").map(String::from),
                display_name: claims.str("name").map(String::from),
                external: vec![external],
                status: AccountStatus::Active,
                roles: Vec::new(),
                teams: groups.clone().unwrap_or_default(),
                scim_id: None,
                external_id: None,
                created_at: now,
                updated_at: now,
            };
            match store.create_user(&user, &actor).await {
                Ok(()) => return Ok(user),
                Err(rill_store::Error::Conflict(_)) => {
                    return Err(Error::Denied(format!(
                    "the name {name:?} belongs to another account; an administrator must link it"
                )))
                }
                Err(e) => return Err(e.into()),
            }
        }
    };
    if user.org != provider.org {
        return Err(Error::Denied(
            "this identity belongs to another organization".into(),
        ));
    }
    if !user.is_active() {
        return Err(Error::Denied(format!(
            "{} is suspended",
            user.principal.name
        )));
    }
    match groups {
        Some(teams) if teams != user.teams => {
            let updated = UserRecord {
                teams,
                updated_at: now,
                ..user.clone()
            };
            store.update_user(&user, &updated, &actor).await?;
            Ok(updated)
        }
        _ => Ok(user),
    }
}
