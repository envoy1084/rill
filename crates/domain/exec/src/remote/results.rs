//! The action cache: an action's digest to the result of running it.

use bytes::Bytes;
use prost::Message;
use rill_reapi_proto::reapi::ActionResult;

use super::digest::BlobDigest;
use super::space::ExecSpace;
use crate::error::{Error, Result};

const RESULT_KIND: &str = "reapi:ac";

impl ExecSpace {
    pub async fn result(&self, action: &BlobDigest) -> Result<Option<ActionResult>> {
        let Some(bytes) = self.records.get_derived(RESULT_KIND, &action.hash).await? else {
            return Ok(None);
        };
        ActionResult::decode(bytes)
            .map(Some)
            .map_err(|e| Error::Codec(format!("action result {action}: {e}")))
    }

    /// Record `result` for `action`. Only workers produce results, so a
    /// client can never poison the cache.
    pub async fn put_result(&self, action: &BlobDigest, result: &ActionResult) -> Result<()> {
        self.records
            .put_derived(
                RESULT_KIND,
                &action.hash,
                Bytes::from(result.encode_to_vec()),
            )
            .await?;
        Ok(())
    }
}
