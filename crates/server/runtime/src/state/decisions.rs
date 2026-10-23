//! The submit decision each landing was allowed by, kept so "why did this
//! land?" has an answer and a candidate policy can be replayed against
//! history.

use bytes::Bytes;
use rill_policy::submit::simulate::RecordedDecision;
use rill_store::RepoStore;
use rill_types::{ChangeId, ChangeVersionHash, TrunkSeq};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// One landing's decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandingDecision {
    pub seq: TrunkSeq,
    pub change: ChangeId,
    pub version: ChangeVersionHash,
    /// BLAKE3 of the policy text, hex; its text is kept under that digest.
    pub policy: String,
    /// Where the policy came from when it was recorded.
    pub source: String,
    pub record: RecordedDecision,
}

fn key(seq: TrunkSeq) -> String {
    format!("landing-decision:{}", seq.0)
}

/// Keep `decision`.
pub async fn store(repo: &RepoStore, decision: &LandingDecision) -> Result<()> {
    let bytes = serde_json::to_vec(decision).map_err(|e| Error::internal(e.to_string()))?;
    repo.set_misc(&key(decision.seq), Bytes::from(bytes))
        .await?;
    Ok(())
}

/// The decision the landing at `seq` was allowed by, when recorded.
pub async fn load(repo: &RepoStore, seq: TrunkSeq) -> Result<Option<LandingDecision>> {
    match repo.get_misc(&key(seq)).await? {
        Some(b) => Ok(Some(
            serde_json::from_slice(&b).map_err(|e| Error::internal(e.to_string()))?,
        )),
        None => Ok(None),
    }
}
