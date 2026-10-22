//! The object store over the [`object_store`] crate, for every backend:
//! the local filesystem (fsynced), S3 and S3-compatible services (feature
//! `s3`), and the in-memory store used in tests.
//!
//! Keys are `<prefix>/<namespace>/<kind>/<hh>/<hash-hex>`, and values are
//! [`codec`](crate::codec) frames passed
//! through the backend's [`Sealer`]. Writes use conditional put
//! (`If-None-Match: *`) so an object is never overwritten. Some
//! S3-compatible services do not support it; [`ObjectStoreBackend::probe`]
//! detects that at startup, and when allowed the backend falls back to
//! checking for the object before an unconditional put, which is safe
//! because content-addressed writes of one key are always identical.

mod access;
mod construct;
mod probe;
mod store;
#[cfg(test)]
mod tests;

pub use store::{ObjectStoreBackend, PutStrategy};
