//! A worker's hold on a job.

use bytes::Bytes;
use rill_store::Condition;
use rill_types::Timestamp;

use crate::job::Job;

/// A claimed job. The claim's token stays the same across renewals, so
/// writes fenced on it fail once the job was taken back, however often
/// the claim was renewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub job: Job,
    pub holder: String,
    pub lease_until: Timestamp,
    pub(crate) token: [u8; 16],
    pub(crate) token_key: Bytes,
    pub(crate) leased_key: Bytes,
    pub(crate) leased_value: Bytes,
}

impl Claim {
    /// A condition that holds while this claim does, for a handler's own
    /// writes.
    pub fn fence(&self) -> Condition {
        Condition::Equals {
            key: self.token_key.clone(),
            value: Some(Bytes::copy_from_slice(&self.token)),
        }
    }
}
