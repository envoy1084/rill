//! An in-process dev server and a runner for `rill` commands against it.

use std::net::SocketAddr;
use std::path::Path;

use rill_api::{Listeners, ServerConfig, StorageConfig};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// A running dev server, and a client daemon serving checkouts.
pub struct Server {
    endpoint: String,
    shutdown: CancellationToken,
    daemon: std::sync::Arc<rill_daemon::Daemon>,
    daemon_dir: tempfile::TempDir,
}

impl Server {
    pub async fn start() -> Self {
        super::isolate_settings();
        let any: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listeners = Listeners::bind(any, any).await.unwrap();
        let (grpc, _http) = listeners.addrs().unwrap();
        let config = ServerConfig {
            dev_mode: true,
            storage: StorageConfig::Memory,
            ..ServerConfig::default()
        };
        let shutdown = CancellationToken::new();
        let state = rill_api::build_state(config, shutdown.clone())
            .await
            .unwrap();
        let token = shutdown.clone();
        tokio::spawn(async move {
            rill_api::serve_with_state(state, listeners, token)
                .await
                .unwrap();
        });
        let daemon_dir = tempfile::tempdir().unwrap();
        let mut config =
            rill_daemon::DaemonConfig::new(rill_daemon::DaemonDir::new(daemon_dir.path()));
        config.cache_budget = 1 << 30;
        // Pushes to server workspaces would make operation logs depend on
        // timing; the daemon's own tests cover them.
        config.sync_delay = std::time::Duration::from_secs(3600);
        let daemon = rill_daemon::Daemon::open(config).await.unwrap();
        tokio::spawn(rill_daemon::serve(daemon.clone()));
        let socket = daemon_dir.path().join("rilld.sock");
        while !socket.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        Server {
            endpoint: format!("http://{grpc}"),
            shutdown,
            daemon,
            daemon_dir,
        }
    }

    /// Run `rill` with `args` (after the global `--endpoint`), returning
    /// the exit code and stdout.
    pub async fn rill(&self, cwd: &Path, args: &[&str]) -> (i32, String) {
        let mut full = vec!["--no-color"];
        full.extend_from_slice(args);
        self.rill_with(cwd, &full).await
    }

    /// Like `rill` but with colour forced on.
    pub async fn rill_colored(&self, cwd: &Path, args: &[&str]) -> (i32, String) {
        let mut full = vec!["--color", "always"];
        full.extend_from_slice(args);
        self.rill_with(cwd, &full).await
    }

    async fn rill_with(&self, cwd: &Path, args: &[&str]) -> (i32, String) {
        let mut full = vec![
            "rill".to_string(),
            "--endpoint".to_string(),
            self.endpoint.clone(),
            "-C".to_string(),
            cwd.display().to_string(),
            "--daemon-dir".to_string(),
            self.daemon_dir.path().display().to_string(),
        ];
        full.extend(args.iter().map(|a| a.to_string()));
        let mut out = Vec::new();
        let code = rill_cli::run_cli_with_args(full, &mut out).await;
        (code, String::from_utf8(out).unwrap())
    }

    /// Like `rill` but asserts success and parses `--json` output.
    pub async fn json(&self, cwd: &Path, args: &[&str]) -> Value {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        let (code, out) = self.rill(cwd, &full).await;
        assert_eq!(code, 0, "{args:?} failed: {out}");
        let doc: Value =
            serde_json::from_str(&out).unwrap_or_else(|e| panic!("{args:?}: {e}: {out}"));
        assert!(
            doc["schema"]
                .as_str()
                .is_some_and(|s| s.starts_with("rill.cli.v1.")),
            "{args:?}: no schema: {out}"
        );
        doc["result"].clone()
    }

    /// Like `json` but returns the whole document, schema included.
    pub async fn document(&self, cwd: &Path, args: &[&str]) -> Value {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        let (code, out) = self.rill(cwd, &full).await;
        assert_eq!(code, 0, "{args:?} failed: {out}");
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("{args:?}: {e}: {out}"))
    }

    /// Like `rill` but asserts success and returns stdout.
    pub async fn ok(&self, cwd: &Path, args: &[&str]) -> String {
        let (code, out) = self.rill(cwd, args).await;
        assert_eq!(code, 0, "{args:?} failed: {out}");
        out
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.daemon.stop();
        self.shutdown.cancel();
    }
}
