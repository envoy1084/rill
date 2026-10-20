//! Durable records of what landed.
//!
//! Each landing is stored twice: under derived kind `land@1` keyed by the
//! landed version hash (point lookup), and in a bounded ring in the `misc`
//! row `landings` (recent listing without a scan).

use bytes::Bytes;
use rill_store::repo::{decode_tagged, encode_tagged};
use rill_store::RepoStore;
use rill_types::{ChangeId, ChangeVersionHash, Principal, RiskClass, Timestamp, TrunkSeq};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Derived-data kind of per-version landing records.
pub const LANDING_KIND: &str = "land@1";
/// Misc row holding the recent-landings ring.
pub const LANDINGS_MISC: &str = "landings";
/// Number of landings kept in the ring.
pub const LANDINGS_RING: usize = 1000;

/// One landed version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Landing {
    pub seq: TrunkSeq,
    pub change_id: ChangeId,
    /// The version that landed (after any rebase the lander performed).
    pub version: ChangeVersionHash,
    /// The version that was requested, when the lander rebased it.
    pub requested_version: ChangeVersionHash,
    pub requester: Principal,
    pub risk: RiskClass,
    pub landed_at: Timestamp,
}

/// Store `landing` under its version and append it to the ring.
///
/// The ring update is a read-modify-write; callers serialize it (the
/// lander holds a lock) so concurrent landings do not drop entries.
pub async fn record_landing(repo: &RepoStore, landing: &Landing) -> Result<()> {
    repo.put_derived(
        LANDING_KIND,
        &landing.version,
        encode(LANDING_KIND, landing)?,
    )
    .await?;
    let mut ring = list_landings(repo).await?;
    ring.push(landing.clone());
    if ring.len() > LANDINGS_RING {
        let excess = ring.len() - LANDINGS_RING;
        ring.drain(..excess);
    }
    repo.set_misc(LANDINGS_MISC, encode(LANDINGS_MISC, &ring)?)
        .await?;
    Ok(())
}

/// The landing record of `version`, if it landed through the lander.
pub async fn landing_of(repo: &RepoStore, version: &ChangeVersionHash) -> Result<Option<Landing>> {
    match repo.get_derived(LANDING_KIND, version).await? {
        Some(bytes) => Ok(Some(decode(LANDING_KIND, &bytes)?)),
        None => Ok(None),
    }
}

/// The most recent landings, oldest first, up to [`LANDINGS_RING`].
pub async fn list_landings(repo: &RepoStore) -> Result<Vec<Landing>> {
    match repo.get_misc(LANDINGS_MISC).await? {
        Some(bytes) => decode(LANDINGS_MISC, &bytes),
        None => Ok(Vec::new()),
    }
}

fn encode<T: Serialize>(tag: &str, value: &T) -> Result<Bytes> {
    encode_tagged(tag, value).map_err(|e| Error::Codec(e.to_string()))
}

fn decode<T: serde::de::DeserializeOwned>(tag: &str, bytes: &[u8]) -> Result<T> {
    decode_tagged(tag, bytes).map_err(|e| Error::Codec(e.to_string()))
}
