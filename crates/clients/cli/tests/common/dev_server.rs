//! An in-process dev server and a runner for `rill` commands against it.

use std::net::SocketAddr;
use std::path::Path;

use rill_api::{Listeners, ServerConfig, StorageConfig};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// A running dev server.
pub struct Server {
    endpoint: String,
    shutdown: CancellationToken,
}

impl Server {
    pub async fn start() -> Self {
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
        Server {
            endpoint: format!("http://{grpc}"),
            shutdown,
        }
    }

    /// Run `rill` with `args` (after the global `--endpoint`), returning
    /// the exit code and stdout.
    pub async fn rill(&self, cwd: &Path, args: &[&str]) -> (i32, String) {
        let mut full = vec![
            "rill".to_string(),
            "--endpoint".to_string(),
            self.endpoint.clone(),
            "-C".to_string(),
            cwd.display().to_string(),
            "--no-color".to_string(),
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
        self.shutdown.cancel();
    }
}
