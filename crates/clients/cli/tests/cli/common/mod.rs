//! An in-process dev server driven through the CLI as a library.

#![allow(dead_code, unused_imports)]

mod approval;
mod dev_server;
pub mod secure;

pub use approval::approve;
pub use dev_server::Server;

/// Keep the developer's own settings file (aliases, colours) out of tests.
pub fn isolate_settings() {
    std::env::set_var(
        rill_cli::settings::CONFIG_ENV,
        "/nonexistent/rill-test-config.toml",
    );
}
