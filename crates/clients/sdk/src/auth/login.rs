//! `login`: the device flow through one of the organization's providers,
//! then the provider's ID token exchanged for a Rill session.

use crate::client::Connection;
use rill_proto::rill::v1 as pb;

use super::device::{DeviceAuthorization, DeviceFlow};
use super::store::Credentials;
use crate::error::{Error, Result};

const TOKEN_TYPE_ID: &str = "urn:ietf:params:oauth:token-type:id_token";

/// Told what the person must do to approve the sign-in.
pub trait LoginPrompt: Send + Sync {
    /// Show the code and where to approve it.
    fn show(&self, authorization: &DeviceAuthorization);
}

/// The providers `org` signs in through.
pub async fn sign_in_providers(
    connection: Connection,
    org: &str,
) -> Result<Vec<pb::SignInProvider>> {
    Ok(connection
        .sessions()
        .list_sign_in_providers(pb::ListSignInProvidersRequest { org: org.into() })
        .await?
        .into_owned()
        .providers)
}

/// Sign in to the server behind `connection` (reached at `endpoint`) through
/// `org`'s provider `provider` (the only one when `None`).
pub async fn login(
    connection: Connection,
    endpoint: &str,
    org: &str,
    provider: Option<&str>,
    prompt: &dyn LoginPrompt,
) -> Result<Credentials> {
    let providers = sign_in_providers(connection.clone(), org).await?;
    let chosen = match provider {
        Some(name) => providers
            .into_iter()
            .find(|p| p.name == name)
            .ok_or_else(|| Error::config(format!("org {org:?} has no provider {name:?}")))?,
        None => match providers.len() {
            1 => providers.into_iter().next().expect("one"),
            0 => {
                return Err(Error::config(format!(
                    "org {org:?} has no identity provider"
                )))
            }
            _ => {
                let names: Vec<_> = providers.iter().map(|p| p.name.as_str()).collect();
                return Err(Error::config(format!(
                    "org {org:?} has several providers ({}); choose one",
                    names.join(", ")
                )));
            }
        },
    };
    let missing = |what: &str| {
        Error::config(format!(
            "provider {:?} has no {what}, so the CLI cannot sign in through it",
            chosen.name
        ))
    };
    let flow = DeviceFlow {
        client_id: chosen
            .cli_client_id
            .clone()
            .ok_or_else(|| missing("CLI client"))?,
        scopes: if chosen.scopes.is_empty() {
            vec!["openid".into(), "profile".into(), "email".into()]
        } else {
            chosen.scopes.clone()
        },
        device_authorization_endpoint: chosen
            .device_authorization_endpoint
            .clone()
            .ok_or_else(|| missing("device authorization endpoint"))?,
        token_endpoint: chosen
            .token_endpoint
            .clone()
            .ok_or_else(|| missing("token endpoint"))?,
    };
    let http = reqwest::Client::new();
    let authorization = flow.start(&http).await?;
    prompt.show(&authorization);
    let id_token = flow.wait(&http, &authorization).await?;
    let grant = connection
        .sessions()
        .exchange_token(pb::ExchangeTokenRequest {
            subject_token: id_token,
            subject_token_type: TOKEN_TYPE_ID.into(),
            org: org.into(),
            agent: None,
            model: None,
        })
        .await?
        .into_owned();
    Ok(Credentials::from_grant(endpoint, grant))
}
