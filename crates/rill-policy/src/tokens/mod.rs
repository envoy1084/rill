//! Capability tokens.
//!
//! A token is a [Biscuit](https://www.biscuitsec.org) signed by the
//! instance's root key. Its authority block names the principal and carries
//! Datalog checks for a [`Capability`]; a holder can [`attenuate`] it offline
//! into a child that grants strictly less. [`Verifier::verify`] checks a
//! token against a concrete [`TokenRequest`] and returns the principal.
//! Tokens travel as base64 strings.

mod capability;
mod datalog;
mod issuer;
#[cfg(test)]
mod tests;
mod verifier;

pub use capability::{Capability, Verb};
pub use issuer::{attenuate, inspect, Issuer};
pub use verifier::{TokenRequest, Verified, Verifier};
