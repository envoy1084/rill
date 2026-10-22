//! The object protocol: plain HTTP access to content-addressed objects.
//!
//! This is the surface that caches and CDNs sit in front of. Every object
//! URL is immutable (`/o/{repo}/{kind}/{hash}` names exactly one byte
//! string forever), responses carry `ETag` and a one-year immutable
//! `Cache-Control`, and reads need only the bearer token and a read
//! relation on the repository, so an edge cache can serve a hot
//! repository's trees and blobs without touching the gRPC API. Writes go
//! through `PUT` with the hash verified server-side.
//!
//! Routes:
//!
//! * `GET|HEAD /o/{repo}/{kind}/{hash}`: one object;
//! * `PUT /o/{repo}/{kind}/{hash}`: store one object (400 on hash mismatch);
//! * `POST /o/{repo}/batch`: many objects, length-prefixed (see [`batch`]);
//! * `GET /redactions`: redacted hashes, for cache tiers;
//! * `GET /healthz`, `GET /readyz`: liveness and readiness, no auth.

pub mod authorize;
pub mod batch;
mod error;
pub mod handlers;
mod health;
mod keys;
mod redactions;
mod router;

pub use error::{error_response, ObjectError};
pub use router::{router, MAX_PUT_BYTES};
