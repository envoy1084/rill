//! This server's view of instance identity: the keys tokens verify with,
//! the issuer it signs with, and what has been revoked. A follower keeps
//! it current from the identity log; maintenance rotates keys and sweeps
//! expired records.

mod callers;
mod follower;
mod maintenance;
mod mint;
mod standing;
mod view;

pub use follower::follow;
pub use maintenance::maintain_forever;
pub use mint::mint_token;
pub use standing::AuthorStanding;
pub use view::IdentityView;
