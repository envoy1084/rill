//! Key layout of the metadata store.
//!
//! Every key is a FoundationDB tuple. Repository rows are
//! `(table, namespace, repo, ...)`, so a repository's rows in one table form
//! one range and a namespace's rows a larger one. Tuple elements are
//! self-delimiting: no name or id is ever a prefix of a longer one's key.

mod instance;
mod line_keys;
mod repo_keys;
pub mod table;
mod tuple;

pub use instance::{
    idem, idem_table, identity_pair, identity_row, identity_table, org, org_list, quota, repo,
    repo_by_name, repo_list, repo_org, store_format,
};
pub use line_keys::LineKeys;
pub use repo_keys::RepoKeys;
pub use tuple::tail;
