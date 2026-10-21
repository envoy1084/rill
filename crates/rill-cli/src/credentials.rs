//! Where the CLI keeps signed-in sessions: the OS keychain, or a file
//! readable only by its owner when `RILL_CREDENTIALS_FILE` names one or the
//! keychain is unusable.

use std::path::PathBuf;
use std::sync::Arc;

use rill_sdk::auth::{CredentialStore, Credentials, FileStore, KeychainStore};

/// Names the credentials file to use instead of the keychain.
pub const CREDENTIALS_FILE_ENV: &str = "RILL_CREDENTIALS_FILE";

/// The keychain, falling back to a file when the keychain fails.
struct Fallback {
    keychain: KeychainStore,
    file: FileStore,
}

impl CredentialStore for Fallback {
    fn load(&self, endpoint: &str) -> rill_sdk::Result<Option<Credentials>> {
        match self.keychain.load(endpoint) {
            Ok(Some(found)) => Ok(Some(found)),
            Ok(None) => self.file.load(endpoint),
            Err(e) => {
                tracing::debug!(error = %e, "keychain unavailable; using the credentials file");
                self.file.load(endpoint)
            }
        }
    }

    fn save(&self, credentials: &Credentials) -> rill_sdk::Result<()> {
        match self.keychain.save(credentials) {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::debug!(error = %e, "keychain unavailable; using the credentials file");
                self.file.save(credentials)
            }
        }
    }

    fn delete(&self, endpoint: &str) -> rill_sdk::Result<()> {
        let keychain = self.keychain.delete(endpoint);
        self.file.delete(endpoint)?;
        keychain.or(Ok(()))
    }
}

fn default_file() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("rill")
        .join("credentials.json")
}

/// The store the CLI uses.
pub fn store() -> Arc<dyn CredentialStore> {
    match std::env::var_os(CREDENTIALS_FILE_ENV).filter(|v| !v.is_empty()) {
        Some(path) => Arc::new(FileStore::new(PathBuf::from(path))),
        None => Arc::new(Fallback {
            keychain: KeychainStore,
            file: FileStore::new(default_file()),
        }),
    }
}
