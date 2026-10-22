//! Per-invocation state: resolved connection settings, the checkout the
//! command runs in (if any), lazily built clients and the output sink.
//!
//! Resolution order for the connection: `--endpoint` / `--token` /
//! `--repo` flags, then the enclosing checkout's `.rill/checkout.toml`,
//! then `RILL_ENDPOINT` / `RILL_TOKEN` / `RILL_REPO`, then the SDK config
//! file (`~/.config/rill/config.toml`).

use std::io::Write;
use std::path::{Path, PathBuf};

use rill_sdk::local::CheckoutMeta;
use rill_sdk::{Checkout, Client, ClientConfig, RepoClient};
use rill_types::Provenance;
use serde_json::Value;

use crate::cli::GlobalOpts;
use crate::error::CliError;
use crate::output::Out;
use crate::Result;

/// Tool name recorded in the provenance of every change the CLI makes.
pub const TOOL_NAME: &str = "rill-cli";

/// Environment variable naming the default repository.
pub const REPO_ENV: &str = "RILL_REPO";

/// State shared by every command in one invocation.
pub struct Ctx<'w> {
    opts: GlobalOpts,
    out: Out<'w>,
    cwd: PathBuf,
    checkout_dir: Option<Option<PathBuf>>,
    meta: Option<CheckoutMeta>,
    client: Option<Client>,
}

impl<'w> Ctx<'w> {
    /// Build a context. `cwd` is where checkout discovery starts.
    pub fn new(opts: GlobalOpts, writer: &'w mut dyn Write, color: bool, cwd: PathBuf) -> Self {
        Ctx {
            opts,
            out: Out::new(writer, color),
            cwd,
            checkout_dir: None,
            meta: None,
            client: None,
        }
    }

    /// The global options.
    pub fn opts(&self) -> &GlobalOpts {
        &self.opts
    }

    /// True with `--json`.
    pub fn json(&self) -> bool {
        self.opts.json
    }

    /// The human output sink.
    pub fn out(&mut self) -> &mut Out<'w> {
        &mut self.out
    }

    /// Print `value` as JSON, or run `human` to render it.
    pub fn emit(
        &mut self,
        value: Value,
        human: impl FnOnce(&mut Out<'w>) -> std::io::Result<()>,
    ) -> Result<()> {
        if self.opts.json {
            let text = serde_json::to_string_pretty(&value)?;
            self.out.line(text)?;
        } else {
            human(&mut self.out)?;
        }
        self.out.flush()?;
        Ok(())
    }

    /// Print one compact JSON line (for streams).
    pub fn emit_json_line(&mut self, value: &Value) -> Result<()> {
        self.out.line(serde_json::to_string(value)?)?;
        self.out.flush()?;
        Ok(())
    }

    /// The directory checkout discovery starts from.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// The enclosing checkout directory, if the command runs inside one.
    pub fn find_checkout(&mut self) -> Option<PathBuf> {
        if self.checkout_dir.is_none() {
            self.checkout_dir = Some(Checkout::find(&self.cwd));
        }
        self.checkout_dir.clone().flatten()
    }

    /// The enclosing checkout directory, or a usage error.
    pub fn require_checkout_dir(&mut self) -> Result<PathBuf> {
        self.find_checkout().ok_or_else(|| {
            CliError::usage(format!(
                "not inside a rill checkout (no .rill/ above {})",
                self.cwd.display()
            ))
        })
    }

    /// The enclosing checkout's metadata, read without connecting.
    pub fn checkout_meta(&mut self) -> Option<&CheckoutMeta> {
        if self.meta.is_none() {
            let dir = self.find_checkout()?;
            match Checkout::read_meta(&dir) {
                Ok(meta) => self.meta = Some(meta),
                Err(e) => {
                    tracing::warn!(dir = %dir.display(), error = %e, "unreadable checkout metadata");
                    return None;
                }
            }
        }
        self.meta.as_ref()
    }

    /// The resolved client configuration.
    pub fn client_config(&mut self) -> Result<ClientConfig> {
        let flag_endpoint = self.opts.endpoint.clone();
        let flag_token = self.opts.token.clone();
        let meta_endpoint = self.checkout_meta().map(|m| m.endpoint.clone());
        let mut config = match flag_endpoint.or(meta_endpoint) {
            Some(endpoint) => {
                let mut c = ClientConfig::new(endpoint);
                c.token = std::env::var(rill_sdk::config::TOKEN_ENV)
                    .ok()
                    .filter(|t| !t.is_empty());
                c
            }
            None => ClientConfig::discover().map_err(|e| {
                CliError::usage(format!(
                    "{e}; pass --endpoint, run inside a checkout, or set RILL_ENDPOINT"
                ))
            })?,
        };
        if let Some(token) = flag_token {
            config.token = Some(token);
        }
        if config.token.is_none() {
            let store = crate::credentials::store();
            match store.load(&config.endpoint) {
                Ok(Some(credentials)) => config = config.with_session(credentials, Some(store)),
                Ok(None) => {}
                Err(e) => tracing::debug!(error = %e, "reading stored credentials failed"),
            }
        }
        config.user_agent = format!("rill-cli/{}", env!("CARGO_PKG_VERSION"));
        Ok(config)
    }

    /// Under a workload credential (`RILL_WORKLOAD_TOKEN_FILE`) and with no
    /// token given, connect now, exchanging it for an agent token, so every
    /// later call carries one.
    pub async fn resolve_workload(&mut self) -> Result<()> {
        let explicit = self.opts.token.is_some()
            || std::env::var(rill_sdk::config::TOKEN_ENV).is_ok_and(|t| !t.is_empty());
        if explicit || self.client.is_some() {
            return Ok(());
        }
        let Some(workload) = rill_sdk::auth::WorkloadConfig::from_env() else {
            return Ok(());
        };
        let mut config = self.client_config()?;
        config.token = None;
        config.session = None;
        config.workload = Some(workload);
        self.client = Some(Client::connect(config).await?);
        Ok(())
    }

    /// A (lazily connecting) client, built once.
    pub fn client(&mut self) -> Result<Client> {
        if let Some(c) = &self.client {
            return Ok(c.clone());
        }
        let config = self.client_config()?;
        tracing::debug!(endpoint = %config.endpoint, "connecting");
        let client = Client::connect_lazy(config)?;
        self.client = Some(client.clone());
        Ok(client)
    }

    /// The repository name the command addresses.
    pub fn repo_name(&mut self) -> Result<String> {
        if let Some(r) = &self.opts.repo {
            return Ok(r.clone());
        }
        if let Some(meta) = self.checkout_meta() {
            return Ok(meta.repo.clone());
        }
        std::env::var(REPO_ENV)
            .ok()
            .filter(|r| !r.is_empty())
            .ok_or_else(|| {
                CliError::usage(
                    "no repository: pass --repo, set RILL_REPO, or run inside a checkout",
                )
            })
    }

    /// A repository client.
    pub fn repo(&mut self) -> Result<RepoClient> {
        let name = self.repo_name()?;
        Ok(self.client()?.repo(name))
    }

    /// The enclosing checkout, opened with a connected client.
    pub async fn checkout(&mut self) -> Result<Checkout> {
        let dir = self.require_checkout_dir()?;
        let client = self.client()?;
        Ok(Checkout::open(dir, client).await?)
    }

    /// Provenance for changes the CLI creates.
    pub fn provenance(&self) -> Provenance {
        Provenance {
            tool: Some(TOOL_NAME.into()),
            ..Provenance::default()
        }
    }
}
