//! Agents authenticating as their workload: a JWT the platform mounts (a
//! Kubernetes projected service account token, a CI OIDC token, a SPIFFE
//! JWT-SVID) exchanged for a short agent token, again before it expires.

use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use crate::client::Connection;
use connectrpc::ErrorCode as Code;
use parking_lot::RwLock;
use rill_proto::rill::v1 as pb;

use super::source::TokenSource;
use crate::error::{Error, Result};

/// Names the file holding the workload's JWT.
pub const WORKLOAD_TOKEN_FILE_ENV: &str = "RILL_WORKLOAD_TOKEN_FILE";
/// The org whose trusted issuer signed it.
pub const ORG_ENV: &str = "RILL_ORG";
/// The agent to act as, when several bindings match.
pub const AGENT_ENV: &str = "RILL_AGENT";
/// The model the agent runs as.
pub const MODEL_ENV: &str = "RILL_MODEL";

const TOKEN_TYPE_JWT: &str = "urn:ietf:params:oauth:token-type:jwt";
/// Exchange again this long before the agent token expires.
const RENEW_AHEAD_MS: i64 = 60_000;

/// What to exchange, and for whom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkloadConfig {
    /// The file the platform keeps the workload's JWT in; read on every
    /// exchange, so rotated tokens are picked up.
    pub token_file: PathBuf,
    /// The org whose trusted issuer signed it.
    pub org: String,
    /// The agent to act as, when several bindings match.
    pub agent: Option<String>,
    /// The model the agent runs as.
    pub model: Option<String>,
}

impl WorkloadConfig {
    /// From `RILL_WORKLOAD_TOKEN_FILE`, `RILL_ORG`, `RILL_AGENT` and
    /// `RILL_MODEL`; `None` without a token file.
    pub fn from_env() -> Option<Self> {
        let var = |name| std::env::var(name).ok().filter(|v: &String| !v.is_empty());
        Some(WorkloadConfig {
            token_file: PathBuf::from(var(WORKLOAD_TOKEN_FILE_ENV)?),
            org: var(ORG_ENV).unwrap_or_else(|| "default".into()),
            agent: var(AGENT_ENV),
            model: var(MODEL_ENV),
        })
    }

    /// Exchange the current workload token for an agent token.
    pub async fn exchange(&self, connection: Connection) -> Result<pb::SessionGrant> {
        let jwt = tokio::fs::read_to_string(&self.token_file)
            .await
            .map_err(|e| Error::config(format!("{}: {e}", self.token_file.display())))?;
        Ok(connection
            .sessions()
            .exchange_token(pb::ExchangeTokenRequest {
                subject_token: jwt.trim().to_string(),
                subject_token_type: TOKEN_TYPE_JWT.into(),
                org: self.org.clone(),
                agent: self.agent.clone(),
                model: self.model.clone(),
            })
            .await?
            .into_owned())
    }
}

/// An agent token kept fresh by exchanging the workload token again.
pub struct WorkloadSource {
    grant: RwLock<pb::SessionGrant>,
}

impl WorkloadSource {
    /// Exchange once, then keep renewing in the background until dropped.
    pub async fn start(config: WorkloadConfig, connection: Connection) -> Result<Arc<Self>> {
        let grant = config.exchange(connection.clone()).await?;
        let source = Arc::new(WorkloadSource {
            grant: RwLock::new(grant),
        });
        tokio::spawn(renew_forever(Arc::downgrade(&source), config, connection));
        Ok(source)
    }
}

impl TokenSource for WorkloadSource {
    fn token(&self) -> Option<String> {
        Some(self.grant.read().access_token.clone())
    }
}

async fn renew_forever(
    source: Weak<WorkloadSource>,
    config: WorkloadConfig,
    connection: Connection,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let Some(due_in) = source.upgrade().map(|s| {
            let now = rill_types::Timestamp::now().millis();
            (s.grant.read().expires_at - RENEW_AHEAD_MS - now).max(0)
        }) else {
            return;
        };
        tokio::time::sleep(Duration::from_millis(due_in as u64)).await;
        if source.upgrade().is_none() {
            return;
        }
        match config.exchange(connection.clone()).await {
            Ok(grant) => {
                match source.upgrade() {
                    Some(s) => *s.grant.write() = grant,
                    None => return,
                }
                backoff = Duration::from_secs(1);
            }
            Err(Error::Status {
                code: Code::Unavailable,
                ..
            })
            | Err(Error::Config(_)) => {
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
            Err(e) => {
                tracing::warn!(error = %e, "the workload can no longer act as its agent");
                return;
            }
        }
    }
}
