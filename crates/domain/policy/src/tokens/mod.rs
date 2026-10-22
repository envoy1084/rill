//! Capability tokens.
//!
//! A token is a [Biscuit](https://www.biscuitsec.org) signed by one of the
//! instance's P-256 keys, named by key id. Its authority block names the
//! principal, carries signed [`TokenClaims`] and, when narrowed, Datalog
//! checks for a [`Capability`]; a holder can [`attenuate`] it offline into a
//! child that grants strictly less. [`Verifier::verify`] checks a
//! token against a concrete [`TokenRequest`] and returns the principal.
//! Tokens travel as base64 strings.

mod capability;
mod claims;
mod datalog;
mod issuer;
mod keyring;
#[cfg(test)]
mod tests;
mod verifier;

pub use capability::{Capability, Verb};
pub use claims::{TokenClaims, TokenLifetimes};
pub use issuer::{attenuate, inspect, Issuer};
pub use keyring::{KeyRing, SigningKey, VerifyingKey};
pub use verifier::{TokenRequest, Verified, Verifier};
