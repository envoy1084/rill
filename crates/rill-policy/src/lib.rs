//! Policy for Rill: who may do what, and how risky a change is.
//!
//! * [`risk`]: deterministic risk classification of a change from its facts.
//! * [`submit`]: Cedar submit rules deciding whether a change may land.
//! * [`rebac`]: relationship-based access control over repository paths.
//! * [`tokens`]: Biscuit capability tokens that agents carry and attenuate.
//! * [`signing`]: P-256 signatures over object hashes.
//! * [`identity`]: turning request credentials into an [`AuthContext`].
//!
//! Nothing here performs I/O except the ReBAC authorizer, which reads
//! relationship tuples from a [`rill_store::RepoStore`].

pub mod error;
pub mod identity;
pub mod rebac;
pub mod risk;
pub mod signing;
pub mod submit;
pub mod tokens;

pub use error::{Error, Result};
pub use identity::{
    AuthContext, AuthHeaders, Authenticator, NoTeams, StaticTokenAuthenticator, TeamResolver,
};
pub use rebac::{Authorizer, PathFilter, Relation, Subject};
pub use risk::{
    classify, ChangeFacts, CompiledRiskConfig, PathFacts, RiskAssessment, RiskConfig, SensitiveGlob,
};
pub use signing::{verify as verify_signature, KeyFile, Signer};
pub use submit::{SubmitDecision, SubmitFacts, SubmitPolicy, VoteSummary};
pub use tokens::{
    attenuate, Capability, Issuer, KeyRing, SigningKey, TokenClaims, TokenLifetimes, TokenRequest,
    Verb, Verified, Verifier,
};
