//! Bearer tokens for requests: fixed, or from a session that refreshes
//! itself before its access token expires.

use std::sync::{Arc, Weak};
use std::time::Duration;

use parking_lot::RwLock;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::session_service_client::SessionServiceClient;
use tonic::transport::Channel;
use tonic::Code;

use super::store::{CredentialStore, Credentials};

/// Hands out the bearer token for the next request.
pub trait TokenSource: Send + Sync {
    /// The token to send now, if any.
    fn token(&self) -> Option<String>;
}

/// A token that never changes.
#[derive(Debug, Clone)]
pub struct StaticToken(pub String);

impl TokenSource for StaticToken {
    fn token(&self) -> Option<String> {
        Some(self.0.clone())
    }
}

/// Refresh this long before the access token expires.
const REFRESH_AHEAD_MS: i64 = 60_000;

/// A signed-in session. A background task refreshes the access token ahead
/// of its expiry and saves each rotated refresh token to the store; it
/// stops when the source is dropped or the server refuses the refresh.
pub struct SessionSource {
    credentials: RwLock<Credentials>,
}

impl SessionSource {
    /// Start refreshing `credentials` over `channel`, saving rotations to
    /// `store`. Needs a tokio runtime.
    pub fn start(
        credentials: Credentials,
        channel: Channel,
        store: Option<Arc<dyn CredentialStore>>,
    ) -> Arc<Self> {
        let source = Arc::new(SessionSource {
            credentials: RwLock::new(credentials),
        });
        tokio::spawn(refresh_forever(
            Arc::downgrade(&source),
            SessionServiceClient::new(channel),
            store,
        ));
        source
    }

    /// The current credentials.
    pub fn credentials(&self) -> Credentials {
        self.credentials.read().clone()
    }
}

impl TokenSource for SessionSource {
    fn token(&self) -> Option<String> {
        Some(self.credentials.read().access_token.clone())
    }
}

async fn refresh_forever(
    source: Weak<SessionSource>,
    mut client: SessionServiceClient<Channel>,
    store: Option<Arc<dyn CredentialStore>>,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let Some(due_in) = source.upgrade().map(|s| {
            let creds = s.credentials.read();
            let now = rill_types::Timestamp::now().millis();
            (creds.expires_at - REFRESH_AHEAD_MS - now).max(0)
        }) else {
            return;
        };
        tokio::time::sleep(Duration::from_millis(due_in as u64)).await;
        let Some(current) = source.upgrade().map(|s| s.credentials()) else {
            return;
        };
        let Some(refresh_token) = current.refresh_token.clone() else {
            return;
        };
        match client
            .refresh_session(pb::RefreshSessionRequest { refresh_token })
            .await
        {
            Ok(grant) => {
                let next = current.renewed(grant.into_inner());
                if let Some(store) = &store {
                    if let Err(e) = store.save(&next) {
                        tracing::warn!(error = %e, "saving refreshed credentials failed");
                    }
                }
                match source.upgrade() {
                    Some(s) => *s.credentials.write() = next,
                    None => return,
                }
                backoff = Duration::from_secs(1);
            }
            Err(status) if status.code() == Code::Unavailable => {
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
            Err(status) => {
                tracing::warn!(error = %status, "the session cannot be refreshed; sign in again");
                return;
            }
        }
    }
}
