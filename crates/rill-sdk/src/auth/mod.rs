//! Credentials: where the bearer token on each request comes from, signing
//! in through an organization's identity provider with the device flow, and
//! keeping the session in the OS keychain.

mod device;
mod login;
mod source;
mod store;

pub use device::{DeviceAuthorization, DeviceFlow};
pub use login::{login, sign_in_providers, LoginPrompt};
pub use source::{SessionSource, StaticToken, TokenSource};
pub use store::{CredentialStore, Credentials, FileStore, KeychainStore};
