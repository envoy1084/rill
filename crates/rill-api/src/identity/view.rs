//! The in-memory identity view every request is authenticated against.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rill_identity::agents::WorkloadKeys;
use rill_identity::keys::{key_service, KeyManager, KeySet};
use rill_identity::oidc::Issuers;
use rill_policy::{Issuer, TokenLifetimes, Verified, Verifier};
use rill_store::identity::IdentityStore;
use rill_store::Stores;
use rill_types::identity::{IdentityChange, UserRecord};
use rill_types::{Principal, SessionId, Timestamp, TokenId};

use crate::config::IdentityConfig;
use crate::error::{Error, Result};

/// The fewest seconds between reloads forced by a token naming a key this
/// server does not know.
const UNKNOWN_KEY_RELOAD: Duration = Duration::from_secs(1);

/// The principal maintenance and key creation act as.
pub fn system_actor() -> Principal {
    Principal::service("rill")
}

/// Keys and revocations as this server last read them.
pub struct IdentityView {
    store: IdentityStore,
    keys: KeyManager,
    lifetimes: TokenLifetimes,
    set: ArcSwap<KeySet>,
    revoked_tokens: RwLock<HashMap<TokenId, Timestamp>>,
    revoked_sessions: RwLock<HashMap<SessionId, Timestamp>>,
    last_forced_reload: Mutex<Option<Instant>>,
    admins: Vec<String>,
    /// Users by `(org, name)`, read on first use and dropped when the
    /// identity log says they changed. `None` caches "no such user".
    users: DashMap<(String, String), Option<Arc<UserRecord>>>,
    pub(super) callers: super::callers::CallerCaches,
    issuers: Issuers,
    workload_keys: WorkloadKeys,
    config: IdentityConfig,
    /// Objects redacted in each org; refused by every read.
    redacted: dashmap::DashSet<(String, rill_types::ObjectHash)>,
}

impl IdentityView {
    /// Open the view: make sure a signing key exists, then load keys and
    /// revocations.
    pub async fn open(stores: &Stores, config: &IdentityConfig) -> Result<Arc<Self>> {
        let wrapper = key_service(&config.keys).await?;
        let lifetimes = config.lifetimes.lifetimes();
        let keys = KeyManager::new(
            stores.identity(),
            wrapper,
            config.rotation.policy(),
            lifetimes,
        );
        keys.ensure_key(Timestamp::now(), &system_actor()).await?;
        let set = keys.load(Timestamp::now()).await?;
        let issuers = Issuers::new(reqwest::Client::new());
        let view = Arc::new(IdentityView {
            store: stores.identity(),
            keys,
            lifetimes,
            set: ArcSwap::from_pointee(set),
            revoked_tokens: RwLock::new(HashMap::new()),
            revoked_sessions: RwLock::new(HashMap::new()),
            last_forced_reload: Mutex::new(None),
            admins: config.admins.clone(),
            users: DashMap::new(),
            callers: Default::default(),
            issuers: issuers.clone(),
            workload_keys: WorkloadKeys::new(issuers),
            config: config.clone(),
            redacted: dashmap::DashSet::new(),
        });
        view.reload_revocations().await?;
        view.reload_redactions().await?;
        Ok(view)
    }

    pub fn store(&self) -> &IdentityStore {
        &self.store
    }

    pub fn keys(&self) -> &KeyManager {
        &self.keys
    }

    pub fn lifetimes(&self) -> TokenLifetimes {
        self.lifetimes
    }

    pub fn config(&self) -> &IdentityConfig {
        &self.config
    }

    /// OIDC issuers this process has discovered.
    pub fn issuers(&self) -> &Issuers {
        &self.issuers
    }

    /// Key caches of workload issuers.
    pub fn workload_keys(&self) -> &WorkloadKeys {
        &self.workload_keys
    }

    /// The user `name` of `org`, from the cache.
    pub async fn user(&self, org: &str, name: &str) -> Result<Option<Arc<UserRecord>>> {
        let key = (org.to_string(), name.to_string());
        if let Some(cached) = self.users.get(&key) {
            return Ok(cached.clone());
        }
        let user = self.store.user(org, name).await?.map(Arc::new);
        self.users.insert(key, user.clone());
        Ok(user)
    }

    /// When a stored key next activates, if one is pending.
    pub fn next_activation(&self) -> Option<Timestamp> {
        self.set.load().next_activation
    }

    /// The issuer new tokens are signed with.
    pub fn issuer(&self) -> Result<Arc<Issuer>> {
        self.set
            .load()
            .issuer
            .clone()
            .ok_or_else(|| Error::Unavailable("no token signing key is active".into()))
    }

    /// What signs read grants on this server.
    pub fn grant_signer(&self) -> Result<rill_policy::grants::GrantSigner> {
        self.set
            .load()
            .grant_signer
            .clone()
            .ok_or_else(|| Error::Unavailable("no token signing key is active".into()))
    }

    /// What signs audit roots on this server.
    pub fn audit_signer(&self) -> Result<rill_policy::audit::AuditSigner> {
        self.set
            .load()
            .audit_signer
            .clone()
            .ok_or_else(|| Error::Unavailable("no token signing key is active".into()))
    }

    /// Every stored key's public audit key: `(id, key, not_after)`.
    pub fn audit_keys(&self) -> Vec<(u32, Vec<u8>, Option<Timestamp>)> {
        self.set.load().audit_keys.clone()
    }

    /// The public keys read grants verify with.
    pub fn grant_keys(&self) -> rill_policy::grants::GrantKeys {
        self.set.load().grant_keys.clone()
    }

    /// Whether `principal` administers the instance by configuration.
    pub fn is_configured_admin(&self, principal: &Principal) -> bool {
        self.admins.contains(&principal.name)
    }

    /// Reread the keys.
    pub async fn reload_keys(&self) -> Result<()> {
        let set = self.keys.load(Timestamp::now()).await?;
        self.set.store(Arc::new(set));
        Ok(())
    }

    /// Reread every applied redaction.
    pub async fn reload_redactions(&self) -> Result<()> {
        for object in self.store.redacted_objects().await? {
            self.redacted.insert(object);
        }
        Ok(())
    }

    /// Every redacted object: `(org, hash)`.
    pub fn redacted_objects(&self) -> Vec<(String, rill_types::ObjectHash)> {
        self.redacted.iter().map(|e| e.key().clone()).collect()
    }

    /// Reread every revocation still in force.
    pub async fn reload_revocations(&self) -> Result<()> {
        let now = Timestamp::now();
        let tokens = self.store.revocations(now).await?;
        let sessions = self.store.revoked_sessions(now).await?;
        *self.revoked_tokens.write() = tokens
            .into_iter()
            .map(|r| (r.token_id, r.expires_at))
            .collect();
        *self.revoked_sessions.write() =
            sessions.into_iter().map(|s| (s.id, s.expires_at)).collect();
        Ok(())
    }

    /// Reread everything.
    pub async fn reload(&self) -> Result<()> {
        self.users.clear();
        self.callers.clear();
        self.reload_keys().await?;
        self.reload_revocations().await?;
        self.reload_redactions().await
    }

    /// Apply one log entry. Returns whether the keys must be reread.
    pub fn apply(&self, change: &IdentityChange) -> bool {
        match change {
            IdentityChange::KeysChanged => true,
            IdentityChange::TokenRevoked {
                token_id,
                expires_at,
            } => {
                self.revoked_tokens.write().insert(*token_id, *expires_at);
                false
            }
            IdentityChange::SessionRevoked { session } => {
                // The exact end comes with the next full reload; until then
                // the session stays refused.
                self.revoked_sessions
                    .write()
                    .insert(*session, Timestamp::from_millis(i64::MAX));
                false
            }
            IdentityChange::UserChanged { org, name } => {
                self.users.remove(&(org.clone(), name.clone()));
                // A user may sponsor agents, directly or through a team.
                self.callers.forget_org(org);
                false
            }
            IdentityChange::AgentChanged { org, name } => {
                self.callers.forget_agent(org, name);
                false
            }
            IdentityChange::OrgSettingsChanged { org } => {
                self.callers.forget_org(org);
                false
            }
            IdentityChange::ProviderChanged { .. }
            | IdentityChange::WorkloadIssuerChanged { .. }
            | IdentityChange::TeamChanged { .. }
            | IdentityChange::SessionOpened { .. }
            | IdentityChange::TokenIssued { .. }
            | IdentityChange::OrgCreated { .. }
            | IdentityChange::ScimCredentialChanged { .. }
            | IdentityChange::RedactionRequested { .. }
            | IdentityChange::QuarantineCleared { .. } => false,
            IdentityChange::RedactionApplied { org, hash, .. } => {
                self.redacted.insert((org.clone(), *hash));
                false
            }
        }
    }

    /// Verify `token` and refuse it when it, or its session, is revoked.
    pub async fn verify(&self, token: &str) -> Result<Verified> {
        let now = Timestamp::now();
        let verified = match self.check_signature(token, now) {
            Err(rill_policy::Error::Token(msg)) if msg.contains("unknown signing key") => {
                if self.may_force_reload() {
                    self.reload_keys().await?;
                }
                self.check_signature(token, now)
            }
            other => other,
        }
        .map_err(|e| match e {
            rill_policy::Error::TokenExpired { .. } => Error::Policy(e),
            other => Error::Unauthenticated(other.to_string()),
        })?;
        if self
            .revoked_tokens
            .read()
            .contains_key(&verified.claims.token_id)
        {
            return Err(Error::Unauthenticated("token revoked".into()));
        }
        if let Some(session) = verified.claims.session {
            if self.revoked_sessions.read().contains_key(&session) {
                return Err(Error::Unauthenticated("session revoked".into()));
            }
        }
        if let Some(user) = self
            .user(&verified.claims.org, &verified.principal.name)
            .await?
        {
            if user.principal.kind == verified.principal.kind && !user.is_active() {
                return Err(Error::Unauthenticated(format!(
                    "{} is suspended",
                    verified.principal
                )));
            }
        }
        Ok(verified)
    }

    /// Whether `session` is revoked, as this server last heard.
    pub fn session_revoked(&self, session: &SessionId) -> bool {
        self.revoked_sessions.read().contains_key(session)
    }

    fn check_signature(&self, token: &str, now: Timestamp) -> rill_policy::Result<Verified> {
        Verifier::new(self.set.load().ring.clone()).authenticate(token, now)
    }

    fn may_force_reload(&self) -> bool {
        let mut last = self.last_forced_reload.lock();
        let due = last.is_none_or(|t| t.elapsed() >= UNKNOWN_KEY_RELOAD);
        if due {
            *last = Some(Instant::now());
        }
        due
    }
}

impl rill_store::redact::RedactionSet for IdentityView {
    fn is_redacted(&self, org: &str, hash: &rill_types::ObjectHash) -> bool {
        !self.redacted.is_empty() && self.redacted.contains(&(org.to_string(), *hash))
    }
}
