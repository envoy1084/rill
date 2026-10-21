//! `rill login` and `rill logout`.

use clap::Args;
use rill_proto::rill::v1 as pb;
use rill_sdk::auth::{DeviceAuthorization, LoginPrompt};
use serde_json::json;

use crate::context::Ctx;
use crate::Result;

/// `rill login`.
#[derive(Args, Debug)]
pub struct LoginArgs {
    /// The organization to sign in to.
    #[arg(long, default_value = "default")]
    pub org: String,
    /// The identity provider (required when the org has several).
    #[arg(long)]
    pub provider: Option<String>,
}

struct Stderr;

impl LoginPrompt for Stderr {
    fn show(&self, a: &DeviceAuthorization) {
        match &a.verification_uri_complete {
            Some(uri) => eprintln!("To sign in, open {uri}\n(code {})", a.user_code),
            None => eprintln!(
                "To sign in, open {} and enter the code {}",
                a.verification_uri, a.user_code
            ),
        }
        eprintln!("Waiting for approval…");
    }
}

/// Sign in through the org's identity provider and keep the session.
pub async fn login(ctx: &mut Ctx<'_>, args: LoginArgs) -> Result<()> {
    let mut config = ctx.client_config()?;
    config.token = None;
    config.session = None;
    let endpoint = config.endpoint.clone();
    let client = rill_sdk::Client::connect(config).await?;
    let credentials = rill_sdk::auth::login(
        client.channel(),
        &endpoint,
        &args.org,
        args.provider.as_deref(),
        &Stderr,
    )
    .await?;
    crate::credentials::store().save(&credentials)?;
    ctx.emit(
        json!({
            "principal": credentials.principal,
            "org": credentials.org,
            "session": credentials.session,
            "endpoint": credentials.endpoint,
        }),
        |out| {
            out.line(format!(
                "Signed in to {} as {} ({})",
                credentials.endpoint, credentials.principal, credentials.org
            ))
        },
    )
}

/// End the stored session for the server and forget it.
pub async fn logout(ctx: &mut Ctx<'_>) -> Result<()> {
    let config = ctx.client_config()?;
    let endpoint = config.endpoint.clone();
    let store = crate::credentials::store();
    let Some(credentials) = store.load(&endpoint)? else {
        return ctx.emit(json!({ "signed_out": false }), |out| {
            out.line(format!("Not signed in to {endpoint}"))
        });
    };
    let client = ctx.client()?;
    let ended = client
        .identity_svc()
        .revoke_session(pb::RevokeSessionRequest {
            operation_id: Default::default(),
            session: credentials.session.clone(),
        })
        .await;
    if let Err(e) = &ended {
        tracing::warn!(error = %e, "ending the session on the server failed");
    }
    store.delete(&endpoint)?;
    ctx.emit(
        json!({ "signed_out": true, "session_ended": ended.is_ok() }),
        |out| out.line(format!("Signed out of {endpoint}")),
    )
}
