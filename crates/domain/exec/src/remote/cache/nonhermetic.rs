//! Labels whose actions a probe found non-hermetic: their results are not
//! cached until the flag expires.

use std::time::Duration;

use rill_store::repo::{decode_record, encode_record};
use rill_store::KeyRange;
use rill_types::{ObjectHash, Record, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::remote::{ExecSpace, Scheduler};

/// How long a label stays uncached after a mismatch.
pub const NONHERMETIC_FOR: Duration = Duration::from_secs(7 * 24 * 3600);

/// A label whose re-run gave different outputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nonhermetic {
    /// What the action belongs to: `verify:<repo>/<step>`, or a build
    /// tool's target and mnemonic.
    pub label: String,
    /// The action whose two runs differed.
    pub action: ObjectHash,
    /// What differed, one line each.
    pub differences: Vec<String>,
    pub found_at: Timestamp,
    /// Results are cached again after this.
    pub until: Timestamp,
}

impl Record for Nonhermetic {
    const TAG: &'static str = "exec-nonhermetic";
}

impl Scheduler {
    /// The flag on `label`, while it lasts.
    pub async fn nonhermetic(&self, space: &ExecSpace, label: &str) -> Result<Option<Nonhermetic>> {
        let key = space.records.keys().exec_nonhermetic(label);
        let Some(bytes) = space.records.meta().get(&key).await? else {
            return Ok(None);
        };
        let flag: Nonhermetic = decode_record(&bytes)?;
        Ok((flag.until > Timestamp::now()).then_some(flag))
    }

    /// The flags in force whose label starts with `prefix`.
    pub async fn nonhermetics(&self, space: &ExecSpace, prefix: &str) -> Result<Vec<Nonhermetic>> {
        let rows = space
            .records
            .meta()
            .scan(KeyRange::prefix(space.records.keys().exec_nonhermetics()))
            .await?;
        let now = Timestamp::now();
        let mut out = Vec::new();
        for (_, value) in rows {
            let flag: Nonhermetic = decode_record(&value)?;
            if flag.until > now && flag.label.starts_with(prefix) {
                out.push(flag);
            }
        }
        Ok(out)
    }

    pub(crate) async fn flag_nonhermetic(
        &self,
        space: &ExecSpace,
        label: &str,
        action: ObjectHash,
        differences: Vec<String>,
    ) -> Result<()> {
        let now = Timestamp::now();
        let flag = Nonhermetic {
            label: label.to_string(),
            action,
            differences,
            found_at: now,
            until: Timestamp::from_millis(now.millis() + NONHERMETIC_FOR.as_millis() as i64),
        };
        space
            .records
            .meta()
            .set(
                space.records.keys().exec_nonhermetic(label),
                encode_record(&flag)?,
            )
            .await?;
        Ok(())
    }
}
