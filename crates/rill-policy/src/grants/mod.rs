//! Signed read grants: which objects a principal may fetch by hash.
//!
//! A server that has checked a caller may read a path returns, with the
//! listing or diff, a grant naming the objects reached through it. Any tier
//! (the server's object protocol, a regional cache) serves an object only
//! to a request carrying an unexpired grant that names it, and verifies the
//! grant with public keys alone. A hash learned some other way fetches
//! nothing.

mod grant;
mod keys;

pub use grant::{GrantEntry, ReadGrant, GRANT_HEADER, MAX_GRANT_TTL_MS};
pub use keys::{GrantKeys, GrantSigner};
