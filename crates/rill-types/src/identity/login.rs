//! A browser sign-in between the redirect to the provider and its return.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::time::Timestamp;

/// What the server remembers about a sign-in it started, keyed by the
/// `state` parameter. Taken (read and deleted) by the callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingLogin {
    pub state: String,
    pub org: String,
    pub provider: String,
    pub nonce: String,
    pub pkce_verifier: String,
    /// Where to send the browser afterwards; a path on this server.
    pub return_to: Option<String>,
    pub expires_at: Timestamp,
}

impl Record for PendingLogin {
    const TAG: &'static str = "pending-login";
}
