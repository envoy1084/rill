//! `rill dev` and `rill api`: running the service.
//!
//! `rill dev` runs every role in one process for a laptop: no
//! authentication, storage in a local directory (or memory), fixed ports.
//! `rill api` runs a production process from a configuration file and
//! `RILL_*` variables, with `--roles` choosing what it does; any number of
//! such processes may share one store.

mod args;
pub mod cache;
mod config;
mod principal;
mod run;
mod template;

pub use args::{ApiArgs, ApiCmd, DevArgs};
pub use config::{api_config, dev_config, parse_storage, DEV_GRPC_PORT, DEV_HTTP_PORT};
pub use principal::parse_principal;
pub use run::{api, dev};
pub use template::annotated_config;
