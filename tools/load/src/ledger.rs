//! Work the cluster acknowledged, checked at the end of a run: every
//! version a server said it stored and every landing it reported must
//! still be there, whatever was killed in between.

use parking_lot::Mutex;
use rill_sdk::RepoClient;
use rill_types::{ChangeVersionHash, TrunkSeq};
use serde_json::{json, Value};

/// Acknowledged versions and landings.
#[derive(Default)]
pub struct Ledger {
    versions: Mutex<Vec<ChangeVersionHash>>,
    landings: Mutex<Vec<(TrunkSeq, ChangeVersionHash)>>,
}

impl Ledger {
    pub fn version(&self, hash: ChangeVersionHash) {
        self.versions.lock().push(hash);
    }

    pub fn landing(&self, seq: TrunkSeq, hash: ChangeVersionHash) {
        self.landings.lock().push((seq, hash));
    }

    /// Read everything back: how many acknowledged versions and landings
    /// are missing or changed.
    pub async fn verify(&self, repo: &RepoClient) -> Value {
        let versions = self.versions.lock().clone();
        let landings = self.landings.lock().clone();
        let mut lost_versions = Vec::new();
        for hash in &versions {
            if repo.change_info(*hash).await.is_err() {
                lost_versions.push(hash.to_hex());
            }
        }
        let mut lost_landings = Vec::new();
        if let Some(last) = landings.iter().map(|(s, _)| *s).max() {
            let trunk = repo
                .trunk_range(TrunkSeq::ZERO, last)
                .await
                .unwrap_or_default();
            for (seq, hash) in &landings {
                let on_trunk = trunk.iter().any(|e| e.seq == *seq && e.hash == *hash);
                if !on_trunk {
                    lost_landings.push(seq.get());
                }
            }
        }
        json!({
            "acknowledged_versions": versions.len(),
            "acknowledged_landings": landings.len(),
            "lost_versions": lost_versions,
            "lost_landings": lost_landings,
        })
    }
}
