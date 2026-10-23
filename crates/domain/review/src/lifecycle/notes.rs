//! Notes on a review: small keyed records other services keep with it
//! (escalations, audit sampling), stored in `ReviewState::extra`.

use rill_types::{ChangeId, Principal, ReviewState};

use crate::error::Result;
use crate::service::ReviewService;

impl ReviewService {
    /// Set note `key` to `value` on the change's review.
    pub async fn set_note(
        &self,
        change_id: &ChangeId,
        actor: &Principal,
        key: &str,
        value: &str,
    ) -> Result<ReviewState> {
        let (state, ()) = self
            .update(change_id, actor, |s| {
                s.extra.insert(key.to_string(), value.to_string());
                Ok(())
            })
            .await?;
        Ok(state)
    }
}

/// The notes whose keys start with `prefix`, with the prefix removed.
pub fn notes<'a>(
    state: &'a ReviewState,
    prefix: &'a str,
) -> impl Iterator<Item = (&'a str, &'a str)> {
    state
        .extra
        .iter()
        .filter_map(move |(k, v)| k.strip_prefix(prefix).map(|k| (k, v.as_str())))
}
