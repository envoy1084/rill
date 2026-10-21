//! `rill admin idp|user|key|revoke`: identity providers, users, token keys
//! and revocation.

pub mod agents;
pub mod keys;
pub mod providers;
pub mod users;

use clap::{Args, Subcommand};

pub use agents::{AgentCmd, KillArgs, TrustCmd};
pub use keys::KeyCmd;
pub use providers::IdpCmd;
pub use users::UserCmd;

use crate::context::Ctx;
use crate::Result;

/// `rill admin revoke <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum RevokeCmd {
    /// Revoke a token by id (`tk_…`), or the token given with `--token`.
    Token(RevokeTokenArgs),
    /// End a session (`ss_…`): every token issued under it stops working.
    Session(RevokeSessionArgs),
}

/// `rill admin revoke token`.
#[derive(Args, Debug)]
pub struct RevokeTokenArgs {
    /// The token id; without it, the token this command authenticates with.
    pub token_id: Option<String>,
    /// Why, for the record.
    #[arg(long, default_value = "")]
    pub reason: String,
}

/// `rill admin revoke session`.
#[derive(Args, Debug)]
pub struct RevokeSessionArgs {
    /// The session id.
    pub session: String,
}

/// Run `rill admin revoke`.
pub async fn revoke(ctx: &mut Ctx<'_>, cmd: RevokeCmd) -> Result<()> {
    keys::revoke(ctx, cmd).await
}
