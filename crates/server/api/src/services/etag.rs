//! Etags for records stored whole: a digest of the record, checked before
//! a conditional write.

use buffa::Message;

use rill_runtime::error::{Error, Result};

/// The etag of `record` as returned.
pub fn of<M: Message>(record: &M) -> String {
    let digest = blake3::hash(&record.encode_to_vec());
    hex::encode(&digest.as_bytes()[..8])
}

/// Refuse a write unless `given` is `current`; an empty etag is no
/// condition.
pub fn check(current: &str, given: &str, what: &str) -> Result<()> {
    if !given.is_empty() && given != current {
        return Err(Error::EtagMismatch(format!(
            "{what} has changed; read it again"
        )));
    }
    Ok(())
}
