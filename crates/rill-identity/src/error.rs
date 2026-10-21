use thiserror::Error;

/// Errors produced by identity operations.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] rill_store::Error),

    #[error(transparent)]
    Policy(#[from] rill_policy::Error),

    /// A key service refused or failed a request.
    #[error("{backend} key service: {message}")]
    KeyService { backend: String, message: String },

    /// An OpenID Connect provider or token was unusable.
    #[error("OIDC: {0}")]
    Oidc(String),

    /// Identity configuration is unusable.
    #[error("identity configuration: {0}")]
    Config(String),
}

impl Error {
    pub(crate) fn key_service(backend: &str, message: impl std::fmt::Display) -> Self {
        Error::KeyService {
            backend: backend.to_string(),
            message: message.to_string(),
        }
    }
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
