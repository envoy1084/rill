use thiserror::Error;

/// Errors produced by policy evaluation, authorization and token handling.
#[derive(Debug, Error)]
pub enum Error {
    /// A glob in a risk or path configuration failed to compile.
    #[error("invalid glob {glob:?}: {reason}")]
    InvalidGlob { glob: String, reason: String },

    /// The Cedar schema or policy text failed to parse.
    #[error("policy parse error: {0}")]
    PolicyParse(String),

    /// The Cedar policies do not type-check against the schema.
    #[error("policy validation failed: {0}")]
    PolicyValidation(String),

    /// Building Cedar entities or a request failed.
    #[error("policy evaluation error: {0}")]
    PolicyEvaluation(String),

    /// A capability token could not be built, parsed or verified.
    #[error("token error: {0}")]
    Token(String),

    /// The token's checks rejected the request.
    #[error("token rejected: {0}")]
    TokenRejected(String),

    /// The token or capability has expired.
    #[error("token expired at {expires_at}")]
    TokenExpired { expires_at: rill_types::Timestamp },

    /// An attenuation tried to grant more than its parent holds.
    #[error("attenuation would widen the capability: {0}")]
    Widening(String),

    /// A key could not be decoded.
    #[error("invalid key: {0}")]
    InvalidKey(String),

    /// No usable credentials were presented.
    #[error("unauthenticated: {0}")]
    Unauthenticated(String),

    /// The relationship store could not be read.
    #[error(transparent)]
    Store(#[from] rill_store::Error),

    /// A core type failed validation.
    #[error(transparent)]
    Types(#[from] rill_types::Error),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
