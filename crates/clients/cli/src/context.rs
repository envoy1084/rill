//! Per-invocation state: resolved connection settings, the checkout the
//! command runs in (if any), lazily built clients and the output sink.
//!
//! Resolution order for the connection: `--endpoint` / `--token` /
//! `--repo` flags, then the enclosing checkout's marker
//! (`.rill/mount.json`), then `RILL_ENDPOINT` / `RILL_TOKEN` /
//! `RILL_REPO`, then the SDK config file (`~/.config/rill/config.toml`).

use std::io::Write;
use std::path::{Path, PathBuf};

use rill_daemon::proto as dpb;
use rill_daemon::{DaemonClient, DaemonDir, Marker};
use rill_sdk::{Client, ClientConfig, RepoClient};
use rill_types::{ChangeId, Provenance, WorkspaceId};
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
    checkout: Option<Option<(PathBuf, Marker)>>,
    daemon: Option<DaemonClient>,
    client: Option<Client>,
    schema: String,
}

impl<'w> Ctx<'w> {
    /// Build a context. `cwd` is where checkout discovery starts.
    /// `schema` names the JSON the command prints (`rill.cli.v1.log`).
    pub fn new(
        opts: GlobalOpts,
        writer: &'w mut dyn Write,
        color: bool,
        cwd: PathBuf,
        schema: String,
    ) -> Self {
        Ctx {
            opts,
            out: Out::new(writer, color),
            cwd,
            checkout: None,
            daemon: None,
            client: None,
            schema,
        }
    }

    /// The global options.
    pub fn opts(&self) -> &GlobalOpts {
        &self.opts
    }

    /// True with `--json` or `--json-schema`.
    pub fn json(&self) -> bool {
        self.opts.json || self.opts.json_schema.is_some()
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
        if self.json() {
            let envelope = serde_json::json!({ "schema": self.schema, "result": value });
            self.out.line(serde_json::to_string_pretty(&envelope)?)?;
        } else {
            human(&mut self.out)?;
        }
        self.out.flush()?;
        Ok(())
    }

    /// Print one compact JSON line of a stream, as `<schema>.event`.
    pub fn emit_json_line(&mut self, value: &Value) -> Result<()> {
        let envelope =
            serde_json::json!({ "schema": format!("{}.event", self.schema), "event": value });
        self.out.line(serde_json::to_string(&envelope)?)?;
        self.out.flush()?;
        Ok(())
    }

    /// The directory checkout discovery starts from.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// The enclosing checkout directory, if the command runs inside one.
    pub fn find_checkout(&mut self) -> Option<PathBuf> {
        self.checkout_found().map(|(dir, _)| dir)
    }

    /// The enclosing checkout's marker, read without connecting.
    pub fn marker(&mut self) -> Option<Marker> {
        self.checkout_found().map(|(_, marker)| marker)
    }

    fn checkout_found(&mut self) -> Option<(PathBuf, Marker)> {
        if self.checkout.is_none() {
            self.checkout = Some(Marker::find(&self.cwd));
        }
        self.checkout.clone().flatten()
    }

    /// The enclosing checkout directory, or a usage error.
    pub fn require_checkout_dir(&mut self) -> Result<PathBuf> {
        self.find_checkout().ok_or_else(|| {
            CliError::usage(format!(
                "not inside a rill checkout (no .rill/mount.json above {})",
                self.cwd.display()
            ))
        })
    }

    /// The daemon directory: `--daemon-dir`, else the checkout's daemon,
    /// else `$RILL_DAEMON_DIR` or the per-user default.
    pub fn daemon_dir(&mut self) -> Result<DaemonDir> {
        if let Some(dir) = &self.opts.daemon_dir {
            return Ok(DaemonDir::new(dir));
        }
        if let Some(marker) = self.marker() {
            return Ok(DaemonDir::new(marker.daemon));
        }
        Ok(DaemonDir::from_env()?)
    }

    /// The daemon, started in the background if it is not running.
    pub async fn daemon(&mut self) -> Result<DaemonClient> {
        if let Some(d) = &self.daemon {
            return Ok(d.clone());
        }
        let dir = self.daemon_dir()?;
        let exe = std::env::current_exe()?;
        let daemon = rill_daemon::connect_or_start(&dir, &exe, &["daemon", "run"]).await?;
        self.daemon = Some(daemon.clone());
        Ok(daemon)
    }

    /// The token to hand the daemon for this endpoint: `--token` or
    /// `$RILL_TOKEN` (signed-in sessions it reads itself).
    pub fn daemon_token(&self) -> String {
        self.opts
            .token
            .clone()
            .or_else(|| std::env::var(rill_sdk::config::TOKEN_ENV).ok())
            .unwrap_or_default()
    }

    /// The enclosing checkout's mount, from the daemon.
    pub async fn mount(&mut self) -> Result<dpb::Mount> {
        let marker = self.marker().ok_or_else(|| {
            CliError::usage(format!(
                "not inside a rill checkout (no .rill/mount.json above {})",
                self.cwd.display()
            ))
        })?;
        let token = self.daemon_token();
        let mount = self
            .daemon()
            .await?
            .get_mount(dpb::GetMountRequest {
                name: marker.mount,
                token,
                ..Default::default()
            })
            .await?
            .mount
            .into_option()
            .ok_or_else(|| CliError::Other(anyhow::anyhow!("the daemon returned no mount")))?;
        Ok(mount)
    }

    /// The checkout's working change, if the command runs in a checkout
    /// that has one.
    pub async fn working_change(&mut self) -> Result<Option<ChangeId>> {
        if self.find_checkout().is_none() {
            return Ok(None);
        }
        let change = self.mount().await?.working_change;
        Ok(if change.is_empty() {
            None
        } else {
            Some(change.parse()?)
        })
    }

    /// The checkout's server workspace, if it has one.
    pub async fn checkout_workspace(&mut self) -> Result<Option<WorkspaceId>> {
        if self.find_checkout().is_none() {
            return Ok(None);
        }
        let ws = self.mount().await?.workspace;
        Ok(if ws.is_empty() {
            None
        } else {
            Some(ws.parse()?)
        })
    }

    /// The resolved client configuration.
    pub fn client_config(&mut self) -> Result<ClientConfig> {
        let flag_endpoint = self.opts.endpoint.clone();
        let flag_token = self.opts.token.clone();
        let meta_endpoint = self.marker().map(|m| m.endpoint);
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
        if let Some(marker) = self.marker() {
            return Ok(marker.repo);
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

    /// Provenance for changes the CLI creates.
    pub fn provenance(&self) -> Provenance {
        Provenance {
            tool: Some(TOOL_NAME.into()),
            ..Provenance::default()
        }
    }
}
