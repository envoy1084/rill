//! Keeping a signed-in session between runs.

use std::path::PathBuf;

use rill_proto::rill::v1 as pb;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A signed-in session as stored for one server.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    /// The server these are for.
    pub endpoint: String,
    /// The organization signed in to.
    pub org: String,
    /// The signed-in principal's name.
    pub principal: String,
    /// The session id (`ss_…`).
    pub session: String,
    /// The current access token.
    pub access_token: String,
    /// When the access token expires, in milliseconds since the epoch.
    pub expires_at: i64,
    /// The one-use token that renews the session.
    pub refresh_token: Option<String>,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("endpoint", &self.endpoint)
            .field("principal", &self.principal)
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl Credentials {
    /// The credentials a server granted, for `endpoint`.
    pub fn from_grant(endpoint: &str, grant: pb::SessionGrant) -> Self {
        Credentials {
            endpoint: endpoint.to_string(),
            org: grant.org,
            principal: grant.principal.map(|p| p.name).unwrap_or_default(),
            session: grant.session,
            access_token: grant.access_token,
            expires_at: grant.expires_at,
            refresh_token: grant.refresh_token,
        }
    }

    /// These credentials after a refresh.
    pub fn renewed(&self, grant: pb::SessionGrant) -> Self {
        Credentials {
            access_token: grant.access_token,
            expires_at: grant.expires_at,
            refresh_token: grant.refresh_token.or_else(|| self.refresh_token.clone()),
            ..self.clone()
        }
    }
}

/// Where sessions are kept, one per server endpoint.
pub trait CredentialStore: Send + Sync {
    /// The session kept for `endpoint`.
    fn load(&self, endpoint: &str) -> Result<Option<Credentials>>;
    /// Keep `credentials`, replacing any for the same endpoint.
    fn save(&self, credentials: &Credentials) -> Result<()>;
    /// Forget the session for `endpoint`.
    fn delete(&self, endpoint: &str) -> Result<()>;
}

/// The OS keychain (macOS Keychain, Windows Credential Manager, the Linux
/// kernel keyring).
#[derive(Debug, Default, Clone)]
pub struct KeychainStore;

const SERVICE: &str = "rill";

fn keychain_err(e: keyring::Error) -> Error {
    Error::config(format!("keychain: {e}"))
}

impl CredentialStore for KeychainStore {
    fn load(&self, endpoint: &str) -> Result<Option<Credentials>> {
        let entry = keyring::Entry::new(SERVICE, endpoint).map_err(keychain_err)?;
        match entry.get_password() {
            Ok(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| Error::config(format!("stored credentials: {e}"))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keychain_err(e)),
        }
    }

    fn save(&self, credentials: &Credentials) -> Result<()> {
        let text = serde_json::to_string(credentials)
            .map_err(|e| Error::config(format!("credentials: {e}")))?;
        keyring::Entry::new(SERVICE, &credentials.endpoint)
            .and_then(|entry| entry.set_password(&text))
            .map_err(keychain_err)
    }

    fn delete(&self, endpoint: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, endpoint).map_err(keychain_err)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keychain_err(e)),
        }
    }
}

/// A JSON file readable only by its owner, for hosts without a keychain.
#[derive(Debug, Clone)]
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    /// A store in the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        FileStore { path: path.into() }
    }

    fn read_all(&self) -> Result<Vec<Credentials>> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| Error::config(format!("{}: {e}", self.path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(Error::config(format!("{}: {e}", self.path.display()))),
        }
    }

    fn write_all(&self, all: &[Credentials]) -> Result<()> {
        let io = |e: std::io::Error| Error::config(format!("{}: {e}", self.path.display()));
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let text = serde_json::to_string_pretty(all)
            .map_err(|e| Error::config(format!("credentials: {e}")))?;
        crate::files::write_atomic(&self.path, text.as_bytes(), Some(0o600)).map_err(io)
    }
}

impl CredentialStore for FileStore {
    fn load(&self, endpoint: &str) -> Result<Option<Credentials>> {
        Ok(self
            .read_all()?
            .into_iter()
            .find(|c| c.endpoint == endpoint))
    }

    fn save(&self, credentials: &Credentials) -> Result<()> {
        let mut all = self.read_all()?;
        all.retain(|c| c.endpoint != credentials.endpoint);
        all.push(credentials.clone());
        self.write_all(&all)
    }

    fn delete(&self, endpoint: &str) -> Result<()> {
        let mut all = self.read_all()?;
        all.retain(|c| c.endpoint != endpoint);
        self.write_all(&all)
    }
}
