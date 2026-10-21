//! What the HTTP routes and the session RPCs share: exchanging an ID
//! token for a session, refreshing one, and listing sign-in providers.

use rill_identity::oidc::unverified_issuer;
use rill_identity::signin::{
    open_session, refresh_session, resolve_user, RefreshToken, SessionTokens,
};
use rill_types::identity::SessionKind;
use rill_types::Timestamp;

use super::session_ttl_ms;
use crate::error::{Error, Result};
use crate::state::AppState;

/// A provider a client can sign in through.
#[derive(Debug, Clone)]
pub struct SignInProvider {
    pub org: String,
    pub name: String,
    pub issuer: String,
    pub cli_client_id: Option<String>,
    pub scopes: Vec<String>,
    pub device_authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
}

/// `org`'s providers. Discovery is best effort: a provider that is down is
/// listed without endpoints.
pub async fn sign_in_providers(state: &AppState, org: &str) -> Result<Vec<SignInProvider>> {
    let mut out = Vec::new();
    for p in state.identity.store().providers(org).await? {
        let meta = match p.saml {
            Some(_) => None,
            None => state.identity.issuers().metadata(&p.issuer).await.ok(),
        };
        out.push(SignInProvider {
            org: p.org,
            name: p.name,
            issuer: p.issuer,
            cli_client_id: p.cli_client_id,
            scopes: p.scopes,
            device_authorization_endpoint: meta
                .as_ref()
                .and_then(|m| m.device_authorization_endpoint.clone()),
            token_endpoint: meta.map(|m| m.token_endpoint),
        });
    }
    Ok(out)
}

/// A provider's ID token, for any of its clients, becomes a device
/// session.
pub async fn exchange_id_token(state: &AppState, token: &str) -> Result<SessionTokens> {
    let issuer_url = unverified_issuer(token)?;
    let provider = state
        .identity
        .store()
        .provider_by_issuer(&issuer_url)
        .await?
        .ok_or_else(|| {
            Error::Unauthenticated(format!("no provider trusts issuer {issuer_url:?}"))
        })?;
    if provider.saml.is_some() {
        return Err(Error::Unauthenticated(
            "a SAML provider issues no ID tokens".into(),
        ));
    }
    let claims = state
        .identity
        .issuers()
        .verifier(&provider.issuer, provider.audiences())
        .await?
        .verify(token, None)
        .await?;
    let now = Timestamp::now();
    let user = resolve_user(state.identity.store(), &provider, &claims, now).await?;
    let issuer = state.identity.issuer()?;
    let tokens = open_session(
        state.identity.store(),
        &issuer,
        &user,
        SessionKind::Device,
        session_ttl_ms(state),
        now,
    )
    .await?;
    tracing::info!(user = %user.principal, org = %user.org, session = %tokens.session.id, "signed in by token exchange");
    Ok(tokens)
}

/// Refresh a session, refusing (and ending) it when its user has been
/// suspended since signing in.
pub async fn refresh_live(state: &AppState, token: &RefreshToken) -> Result<SessionTokens> {
    let issuer = state.identity.issuer()?;
    let store = state.identity.store();
    let tokens = refresh_session(store, &issuer, token, Timestamp::now()).await?;
    let session = &tokens.session;
    if let Some(user) = state
        .identity
        .user(&session.org, &session.principal.name)
        .await?
    {
        if !user.is_active() {
            store
                .revoke_session(&session.id, &session.principal)
                .await?;
            return Err(Error::Unauthenticated(format!(
                "{} is suspended",
                session.principal
            )));
        }
    }
    Ok(tokens)
}

/// An agent token for a workload: short, and renewed by exchanging again.
#[derive(Debug, Clone)]
pub struct WorkloadGrant {
    pub access_token: String,
    pub expires_at: Timestamp,
    pub session: rill_types::identity::SessionRecord,
}

/// A trusted workload's JWT becomes a short session of the agent it is
/// bound to, carrying the agent's sponsor and the declared model.
pub async fn exchange_workload(
    state: &AppState,
    token: &str,
    org: &str,
    agent: Option<&str>,
    model: Option<&str>,
) -> Result<WorkloadGrant> {
    let now = Timestamp::now();
    let store = state.identity.store();
    let (agent, _) = rill_identity::agents::exchange_workload(
        store,
        state.identity.workload_keys(),
        org,
        token,
        agent,
        model,
        now,
    )
    .await?;
    let issuer = state.identity.issuer()?;
    let lifetime = issuer.lifetimes().max_for(agent.principal.kind);
    let session = rill_types::identity::SessionRecord {
        id: rill_types::SessionId::new(),
        org: agent.org.clone(),
        principal: agent.principal.clone(),
        kind: SessionKind::Workload,
        created_at: now,
        expires_at: Timestamp::from_millis(now.millis() + lifetime),
        refresh_hash: None,
        revoked_at: None,
        sponsor: Some(agent.sponsor.label()),
        model: model.map(String::from),
    };
    store.create_session(&session).await?;
    let (access_token, expires_at) = rill_identity::signin::access_token(&issuer, &session, now)?;
    tracing::info!(agent = %agent.principal, org = %agent.org, session = %session.id, model = ?model, "workload exchanged for an agent token");
    Ok(WorkloadGrant {
        access_token,
        expires_at,
        session,
    })
}
