//! Notification delivery settings.

use serde::{Deserialize, Serialize};

/// `[notify]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyConfig {
    /// Where email goes: `smtps://user:pass@host:465`, `smtp://host:25`
    /// (STARTTLS), or `file:///dir` (one file per message, for tests).
    /// Email channels fail without it.
    pub smtp_url: Option<String>,
    /// The sender address.
    pub from: String,
    /// Allow `http://` webhook, Slack and Teams URLs (tests and local
    /// setups); otherwise only `https://`.
    pub allow_insecure_urls: bool,
    /// The UTC hour daily digests are sent at.
    pub digest_hour_utc: u8,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        NotifyConfig {
            smtp_url: None,
            from: "rill@localhost".into(),
            allow_insecure_urls: false,
            digest_hour_utc: 8,
        }
    }
}
