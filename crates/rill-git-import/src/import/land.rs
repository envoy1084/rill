//! Landing an imported mainline onto trunk.

use rill_store::RepoStore;
use rill_types::{ChangeVersionHash, TrunkSeq};

use crate::error::{Error, Result};

/// Land `chain` (oldest first, each the first parent of the next) on
/// trunk, skipping versions that already have a sequence number.
///
/// Trunk must currently end where the first unlanded version's parent is;
/// otherwise some other history was landed and importing on top of it
/// would make sequence numbers lie, so this refuses with
/// [`Error::TrunkDiverged`]. Returns the number of versions landed and the
/// resulting trunk head.
pub async fn land_first_parent_history(
    repo: &RepoStore,
    chain: &[(ChangeVersionHash, Option<ChangeVersionHash>)],
) -> Result<(usize, Option<(TrunkSeq, ChangeVersionHash)>)> {
    let mut landed = 0;
    let mut head = repo.trunk_head().await?;
    for (version, first_parent) in chain {
        if repo.trunk_seq_of(version).await?.is_some() {
            continue;
        }
        let expected = head.map(|(_, h)| h);
        if expected != *first_parent {
            return Err(Error::TrunkDiverged {
                trunk: expected,
                next: *version,
            });
        }
        let seq = repo.land_on_trunk(expected, *version).await?;
        tracing::debug!(version = %version, %seq, "landed imported version");
        head = Some((seq, *version));
        landed += 1;
    }
    Ok((landed, head))
}
