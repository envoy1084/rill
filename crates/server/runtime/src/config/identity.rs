//! Token keys, lifetimes and instance administrators.

use rill_identity::keys::{KeyServiceConfig, RotationPolicy};
use rill_policy::TokenLifetimes;
use serde::{Deserialize, Serialize};

/// The `[identity]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IdentityConfig {
    /// The URL browsers reach this service at (`https://rill.example.com`);
    /// sign-in redirects come back to `<public_url>/auth/callback`. Defaults
    /// to the HTTP listen address.
    pub public_url: Option<String>,
    /// How long a sign-in session lasts before signing in again.
    pub session_hours: u64,
    /// The key service that wraps token signing keys.
    pub keys: KeyServiceConfig,
    pub lifetimes: LifetimesConfig,
    pub rotation: RotationConfig,
    /// Principal names that administer the instance (keys, revocation,
    /// organizations) before any role is stored.
    pub admins: Vec<String>,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        IdentityConfig {
            public_url: None,
            session_hours: 12,
            keys: KeyServiceConfig::default(),
            lifetimes: LifetimesConfig::default(),
            rotation: RotationConfig::default(),
            admins: Vec::new(),
        }
    }
}

/// The longest a token may live, per principal kind, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LifetimesConfig {
    pub human_secs: u64,
    pub agent_secs: u64,
    pub service_secs: u64,
}

impl Default for LifetimesConfig {
    fn default() -> Self {
        let d = TokenLifetimes::default();
        LifetimesConfig {
            human_secs: (d.human_ms / 1000) as u64,
            agent_secs: (d.agent_ms / 1000) as u64,
            service_secs: (d.service_ms / 1000) as u64,
        }
    }
}

impl LifetimesConfig {
    pub fn lifetimes(&self) -> TokenLifetimes {
        let ms = |s: u64| (s as i64).saturating_mul(1000);
        TokenLifetimes {
            human_ms: ms(self.human_secs),
            agent_ms: ms(self.agent_secs),
            service_ms: ms(self.service_secs),
        }
    }
}

/// When token keys rotate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RotationConfig {
    pub rotate_after_days: u64,
    pub activation_delay_secs: u64,
    pub grace_secs: u64,
}

impl Default for RotationConfig {
    fn default() -> Self {
        let d = RotationPolicy::default();
        RotationConfig {
            rotate_after_days: (d.rotate_after_ms / 86_400_000) as u64,
            activation_delay_secs: (d.activation_delay_ms / 1000) as u64,
            grace_secs: (d.grace_ms / 1000) as u64,
        }
    }
}

impl RotationConfig {
    pub fn policy(&self) -> RotationPolicy {
        RotationPolicy {
            rotate_after_ms: (self.rotate_after_days as i64).saturating_mul(86_400_000),
            activation_delay_ms: (self.activation_delay_secs as i64).saturating_mul(1000),
            grace_ms: (self.grace_secs as i64).saturating_mul(1000),
            ..RotationPolicy::default()
        }
    }
}
