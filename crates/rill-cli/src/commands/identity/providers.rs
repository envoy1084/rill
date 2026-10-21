//! `rill admin idp`: an organization's identity providers.

use clap::{Args, Subcommand};
use rill_proto::rill::v1 as pb;
use serde_json::json;

use crate::context::Ctx;
use crate::output::Table;
use crate::Result;

/// `rill admin idp <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum IdpCmd {
    /// Add or replace an OpenID Connect provider.
    Add(IdpAddArgs),
    /// The org's providers.
    List(OrgArg),
    /// Remove a provider.
    Remove(IdpRemoveArgs),
}

/// `--org`, defaulting to the default org.
#[derive(Args, Debug)]
pub struct OrgArg {
    /// The organization.
    #[arg(long, default_value = "default")]
    pub org: String,
}

/// `rill admin idp add`.
#[derive(Args, Debug)]
pub struct IdpAddArgs {
    /// A name for the provider within the org.
    pub name: String,
    /// The OIDC issuer URL.
    #[arg(long)]
    pub issuer: String,
    /// The confidential client browsers sign in with.
    #[arg(long)]
    pub client_id: String,
    /// Its secret (wrapped by the key service before it is stored).
    #[arg(long, env = "RILL_IDP_CLIENT_SECRET", hide_env_values = true)]
    pub client_secret: Option<String>,
    /// A public client for `rill login` (the device flow).
    #[arg(long)]
    pub cli_client_id: Option<String>,
    /// Scopes to request (default: openid profile email).
    #[arg(long, value_delimiter = ',')]
    pub scopes: Vec<String>,
    /// The claim naming the user (default: preferred_username).
    #[arg(long)]
    pub username_claim: Option<String>,
    /// A claim listing groups, which become teams.
    #[arg(long)]
    pub groups_claim: Option<String>,
    /// Create users on first sign-in.
    #[arg(long)]
    pub auto_provision: bool,
    /// The organization.
    #[command(flatten)]
    pub org: OrgArg,
}

/// `rill admin idp remove`.
#[derive(Args, Debug)]
pub struct IdpRemoveArgs {
    /// The provider's name.
    pub name: String,
    /// The organization.
    #[command(flatten)]
    pub org: OrgArg,
}

fn info_json(p: &pb::IdentityProviderInfo) -> serde_json::Value {
    json!({
        "org": p.org, "name": p.name, "issuer": p.issuer, "client_id": p.client_id,
        "has_client_secret": p.has_client_secret, "cli_client_id": p.cli_client_id,
        "scopes": p.scopes, "username_claim": p.username_claim,
        "groups_claim": p.groups_claim, "auto_provision": p.auto_provision,
    })
}

/// Run `rill admin idp`.
pub async fn run(ctx: &mut Ctx<'_>, cmd: IdpCmd) -> Result<()> {
    let mut svc = ctx.client()?.identity_svc();
    match cmd {
        IdpCmd::Add(a) => {
            let saved = svc
                .put_identity_provider(pb::PutIdentityProviderRequest {
                    operation_id: Default::default(),
                    provider: Some(pb::IdentityProviderSpec {
                        org: a.org.org,
                        name: a.name,
                        issuer: a.issuer,
                        client_id: a.client_id,
                        client_secret: a.client_secret,
                        cli_client_id: a.cli_client_id,
                        scopes: a.scopes,
                        username_claim: a.username_claim,
                        groups_claim: a.groups_claim,
                        auto_provision: a.auto_provision,
                    }),
                })
                .await?
                .into_inner()
                .provider
                .unwrap_or_default();
            ctx.emit(info_json(&saved), |out| {
                out.line(format!("provider {}/{} saved", saved.org, saved.name))
            })
        }
        IdpCmd::List(o) => {
            let providers = svc
                .list_identity_providers(pb::ListIdentityProvidersRequest { org: o.org })
                .await?
                .into_inner()
                .providers;
            ctx.emit(
                json!(providers.iter().map(info_json).collect::<Vec<_>>()),
                |out| {
                    let mut table = Table::new(&["NAME", "ISSUER", "CLIENT", "CLI CLIENT"]);
                    for p in &providers {
                        table.row(vec![
                            p.name.clone(),
                            p.issuer.clone(),
                            p.client_id.clone(),
                            p.cli_client_id.clone().unwrap_or_default(),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        IdpCmd::Remove(a) => {
            let deleted = svc
                .delete_identity_provider(pb::DeleteIdentityProviderRequest {
                    operation_id: Default::default(),
                    org: a.org.org,
                    name: a.name.clone(),
                })
                .await?
                .into_inner()
                .deleted;
            ctx.emit(json!({ "deleted": deleted }), |out| {
                out.line(if deleted {
                    format!("removed {}", a.name)
                } else {
                    format!("no provider {}", a.name)
                })
            })
        }
    }
}
