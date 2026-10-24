//! Which servers serve which search shards of a repository. A server that
//! serves only some shards heartbeats its entry into one shared record; a
//! search sends every shard it does not serve to a live server that does,
//! and searches the rest itself.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use bytes::Bytes;
use rill_store::RepoStore;
use rill_types::Timestamp;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};

/// The shared record's name.
const RECORD: &str = "search-nodes";
/// How often a serving server renews its entry and rereads its peers.
const HEARTBEAT: Duration = Duration::from_secs(10);
/// How long an entry lives without a heartbeat.
const TTL: Duration = Duration::from_secs(30);
/// Compare-and-swap attempts per heartbeat.
const ATTEMPTS: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Nodes {
    /// By advertised URL.
    nodes: BTreeMap<String, Node>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Node {
    shards: Vec<u16>,
    expires_ms: i64,
}

/// Where a search runs each shard.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShardPlan {
    /// Searched here; `None` for every shard.
    pub local: Option<Vec<usize>>,
    /// Sent to each peer, by URL.
    pub remote: Vec<(String, Vec<usize>)>,
}

/// This server's place among a repository's search servers.
pub struct SearchNodes {
    shards: u16,
    /// Empty: this server serves every shard.
    serve: Vec<u16>,
    url: Arc<OnceLock<String>>,
    peers: parking_lot::Mutex<BTreeMap<String, Vec<u16>>>,
}

impl SearchNodes {
    /// `serve` empty means every shard; `url` is set once the API listens.
    pub fn new(shards: u16, serve: Vec<u16>, url: Arc<OnceLock<String>>) -> SearchNodes {
        SearchNodes {
            shards: shards.max(1),
            serve,
            url,
            peers: Default::default(),
        }
    }

    /// Whether this server serves only some shards.
    pub fn partial(&self) -> bool {
        !self.serve.is_empty()
    }

    /// Where a search runs each shard, from the peers last read.
    pub fn plan(&self) -> ShardPlan {
        plan(
            self.shards,
            &self.serve,
            self.url.get().map(String::as_str),
            &self.peers.lock(),
        )
    }

    /// Renew this server's entry (once it has a URL), drop expired ones,
    /// and keep the live peers for planning.
    pub async fn heartbeat(&self, repo: &RepoStore) -> Result<()> {
        for _ in 0..ATTEMPTS {
            let old = repo.get_misc(RECORD).await?;
            let mut nodes: Nodes = match &old {
                Some(b) => serde_json::from_slice(b).map_err(|e| Error::internal(e.to_string()))?,
                None => Nodes::default(),
            };
            let now = Timestamp::now().millis();
            nodes.nodes.retain(|_, n| n.expires_ms > now);
            if let Some(url) = self.url.get() {
                nodes.nodes.insert(
                    url.clone(),
                    Node {
                        shards: self.serve.clone(),
                        expires_ms: now + TTL.as_millis() as i64,
                    },
                );
            }
            *self.peers.lock() = nodes
                .nodes
                .iter()
                .map(|(url, n)| (url.clone(), n.shards.clone()))
                .collect();
            let new = serde_json::to_vec(&nodes).map_err(|e| Error::internal(e.to_string()))?;
            match repo.set_misc_cas(RECORD, old, Bytes::from(new)).await {
                Ok(()) => return Ok(()),
                Err(e) if e.is_conflict() => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    /// Remove this server's entry, on shutdown.
    pub async fn withdraw(&self, repo: &RepoStore) -> Result<()> {
        let Some(url) = self.url.get() else {
            return Ok(());
        };
        for _ in 0..ATTEMPTS {
            let Some(old) = repo.get_misc(RECORD).await? else {
                return Ok(());
            };
            let mut nodes: Nodes =
                serde_json::from_slice(&old).map_err(|e| Error::internal(e.to_string()))?;
            if nodes.nodes.remove(url).is_none() {
                return Ok(());
            }
            let new = serde_json::to_vec(&nodes).map_err(|e| Error::internal(e.to_string()))?;
            match repo.set_misc_cas(RECORD, Some(old), Bytes::from(new)).await {
                Ok(()) => return Ok(()),
                Err(e) if e.is_conflict() => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

/// Heartbeat until `shutdown`, then withdraw.
pub async fn run(nodes: Arc<SearchNodes>, repo: RepoStore, shutdown: CancellationToken) {
    loop {
        if let Err(e) = nodes.heartbeat(&repo).await {
            tracing::warn!(error = %e, "search node heartbeat failed");
        }
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(HEARTBEAT) => {}
        }
    }
    if let Err(e) = nodes.withdraw(&repo).await {
        tracing::warn!(error = %e, "withdrawing the search node failed");
    }
}

/// Each shard goes to this server when it serves it, else to a peer that
/// does (spread by shard number), else to this server anyway.
fn plan(
    shards: u16,
    serve: &[u16],
    me: Option<&str>,
    peers: &BTreeMap<String, Vec<u16>>,
) -> ShardPlan {
    if serve.is_empty() {
        return ShardPlan::default();
    }
    let mut local = Vec::new();
    let mut remote: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for shard in 0..shards {
        if serve.contains(&shard) {
            local.push(shard as usize);
            continue;
        }
        let holders: Vec<&str> = peers
            .iter()
            .filter(|(url, s)| Some(url.as_str()) != me && s.contains(&shard))
            .map(|(url, _)| url.as_str())
            .collect();
        match holders.get(shard as usize % holders.len().max(1)) {
            Some(url) => remote.entry(url).or_default().push(shard as usize),
            None => local.push(shard as usize),
        }
    }
    ShardPlan {
        local: Some(local),
        remote: remote
            .into_iter()
            .map(|(url, s)| (url.to_string(), s))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peers(list: &[(&str, &[u16])]) -> BTreeMap<String, Vec<u16>> {
        list.iter()
            .map(|(u, s)| (u.to_string(), s.to_vec()))
            .collect()
    }

    #[test]
    fn every_shard_is_planned_exactly_once() {
        let peers = peers(&[
            ("http://a", &[0, 1, 2, 3]),
            ("http://b", &[4, 5]),
            ("http://c", &[4, 5]),
        ]);
        let plan = plan(8, &[0, 1, 2, 3], Some("http://a"), &peers);
        let mut all: Vec<usize> = plan.local.clone().unwrap();
        for (url, s) in &plan.remote {
            assert_ne!(url, "http://a");
            all.extend(s);
        }
        all.sort();
        assert_eq!(all, (0..8).collect::<Vec<_>>());
        assert_eq!(plan.local.unwrap(), vec![0, 1, 2, 3, 6, 7]);
        assert_eq!(plan.remote.len(), 2, "shards 4 and 5 spread over b and c");
    }

    #[test]
    fn a_server_serving_everything_never_fans_out() {
        let peers = peers(&[("http://b", &[0, 1])]);
        assert_eq!(plan(4, &[], None, &peers), ShardPlan::default());
    }
}
