//! Typed access to repositories over the two stores.

mod codec;
mod derived;
mod orgs;
mod quota;
mod records;
mod relations;
mod store;
mod stores;
mod trunk;
mod versions;

pub use codec::{decode_record, decode_tagged, encode_record, encode_tagged};
pub use orgs::{validate_org_name, validate_repo_name, OrgConfig, DEFAULT_ORG};
pub use quota::{day_of, metric, QUOTA_SHARDS};
pub use store::RepoStore;
pub use stores::{RepoConfig, RepoSettings, Stores, STORE_FORMAT};
