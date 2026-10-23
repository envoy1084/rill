//! A dev server, a daemon serving a temporary directory, and helpers.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rill_daemon::proto as pb;
use rill_daemon::{Daemon, DaemonClient, DaemonConfig, DaemonDir};
use rill_sdk::testing::DevServer;
use rill_sdk::{ApplyOptions, Client, Edit, EditBase, RepoClient, Root};
use rill_types::{ChangeVersionHash, RepoPathBuf};

pub struct Fixture {
    pub server: DevServer,
    pub client: Client,
    pub dir: tempfile::TempDir,
    pub daemon: Option<Arc<Daemon>>,
    pub rpc: DaemonClient,
}

pub fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

impl Fixture {
    pub async fn start() -> Fixture {
        let server = DevServer::start().await.unwrap();
        let client = server.client().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (daemon, rpc) = start_daemon(dir.path()).await;
        Fixture {
            server,
            client,
            dir,
            daemon: Some(daemon),
            rpc,
        }
    }

    /// Stop the daemon and start a new one on the same directory.
    pub async fn restart(&mut self) {
        if let Some(old) = self.daemon.take() {
            old.stop();
        }
        // The old daemon's tasks hold the state database until they end.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let (daemon, rpc) = start_daemon(self.dir.path()).await;
        self.daemon = Some(daemon);
        self.rpc = rpc;
    }

    pub fn repo(&self, name: &str) -> RepoClient {
        self.client.repo(name)
    }

    /// A repository with `files` in one version.
    pub async fn repo_with(&self, name: &str, files: &[(&str, &str)]) -> ChangeVersionHash {
        self.client.create_repo(name, None).await.unwrap();
        self.version(name, Root::trunk(), files).await
    }

    /// A version setting `files` on `base`.
    pub async fn version(
        &self,
        repo: &str,
        base: Root,
        files: &[(&str, &str)],
    ) -> ChangeVersionHash {
        let edits = files
            .iter()
            .map(|(p, c)| {
                if c.is_empty() {
                    Edit::delete(path(p))
                } else {
                    Edit::set(path(p), c.to_string())
                }
            })
            .collect();
        self.repo(repo)
            .apply_edits(EditBase::Root(base), edits, ApplyOptions::message("v"))
            .await
            .unwrap()
            .hash
    }

    pub fn checkout_dir(&self, name: &str) -> PathBuf {
        self.dir.path().join("work").join(name)
    }

    /// Check out `repo` at `version` into `name`; returns the mount name.
    pub async fn checkout(&self, repo: &str, version: ChangeVersionHash, name: &str) -> String {
        self.checkout_as(repo, version, name, pb::MountMode::Eager)
            .await
    }

    /// [`Fixture::checkout`] in a given mode.
    pub async fn checkout_as(
        &self,
        repo: &str,
        version: ChangeVersionHash,
        name: &str,
        mode: pb::MountMode,
    ) -> String {
        let resp = self
            .rpc
            .create_mount(pb::CreateMountRequest {
                path: self.checkout_dir(name).display().to_string(),
                endpoint: self.server.endpoint().to_string(),
                repo: repo.into(),
                revision: version.to_hex(),
                mode: mode.into(),
                ..Default::default()
            })
            .await
            .unwrap();
        resp.mount.unwrap().name
    }

    pub async fn cache(&self) -> pb::CacheStats {
        self.rpc
            .get_daemon(pb::GetDaemonRequest::default())
            .await
            .unwrap()
            .cache
            .unwrap()
    }

    pub async fn status(&self, mount: &str) -> Vec<(String, pb::ChangeKind)> {
        self.rpc
            .get_status(pb::GetStatusRequest {
                name: mount.into(),
                prefixes: vec![],
            })
            .await
            .unwrap()
            .changes
            .into_iter()
            .map(|c| (c.path, c.kind.as_known().unwrap()))
            .collect()
    }

    pub async fn mount(&self, mount: &str) -> pb::Mount {
        self.rpc
            .get_mount(pb::GetMountRequest {
                name: mount.into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .mount
            .unwrap()
    }
}

async fn start_daemon(root: &Path) -> (Arc<Daemon>, DaemonClient) {
    let dir = DaemonDir::new(root.join("d"));
    let mut config = DaemonConfig::new(dir.clone());
    config.sync_delay = Duration::from_millis(50);
    config.cache_budget = 1 << 30;
    let daemon = Daemon::open(config).await.unwrap();
    tokio::spawn(rill_daemon::serve(daemon.clone()));
    let mut tries = 0;
    let rpc = loop {
        match DaemonClient::connect(&dir).await {
            Ok(c) => break c,
            Err(e) if tries > 100 => panic!("daemon did not start: {e}"),
            Err(_) => {
                tries += 1;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    };
    (daemon, rpc)
}

pub fn write(dir: &Path, rel: &str, content: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

pub fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}
