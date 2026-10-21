//! Computing background kinds for trunk ahead of reads.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use rill_types::{ChangeVersionHash, TrunkSeq};
use tokio::sync::Mutex;

use super::manager::DeriveManager;
use super::registry::scheduled;
use super::traits::Schedule;
use crate::error::{Error, Result};

/// Misc row holding the last trunk sequence fully derived.
pub const DERIVE_CURSOR_MISC: &str = "derive_cursor";
/// Attempts at a version before the worker gives up on it.
pub const MAX_ATTEMPTS: u32 = 3;

/// Derives every [`Schedule::Background`] kind for each trunk version,
/// in trunk order, resuming after a restart from a stored cursor.
pub struct BackgroundDeriver {
    manager: Arc<DeriveManager>,
    attempts: Mutex<HashMap<ChangeVersionHash, u32>>,
}

/// What one pass did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pass {
    /// Trunk versions completed (or given up on).
    pub versions: usize,
    /// Versions skipped after [`MAX_ATTEMPTS`] failures.
    pub abandoned: usize,
    /// The trunk position derived through.
    pub cursor: TrunkSeq,
}

impl BackgroundDeriver {
    pub fn new(manager: Arc<DeriveManager>) -> Self {
        BackgroundDeriver {
            manager,
            attempts: Mutex::new(HashMap::new()),
        }
    }

    /// The last trunk position fully derived.
    pub async fn cursor(&self) -> Result<TrunkSeq> {
        match self.manager.repo().get_misc(DERIVE_CURSOR_MISC).await? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| Error::Codec("bad derive cursor".into()))?;
                Ok(TrunkSeq(u64::from_be_bytes(arr)))
            }
            None => Ok(TrunkSeq::ZERO),
        }
    }

    async fn store_cursor(&self, seq: TrunkSeq) -> Result<()> {
        self.manager
            .repo()
            .set_misc(
                DERIVE_CURSOR_MISC,
                Bytes::copy_from_slice(&seq.get().to_be_bytes()),
            )
            .await?;
        Ok(())
    }

    /// Derive up to `limit` trunk versions after the cursor. A version
    /// whose derivation fails stops the pass, so it is retried next time;
    /// after [`MAX_ATTEMPTS`] it is logged and skipped.
    pub async fn pass(&self, limit: usize) -> Result<Pass> {
        let repo = self.manager.repo();
        let mut cursor = self.cursor().await?;
        let Some((head, _)) = repo.trunk_head().await? else {
            return Ok(Pass {
                cursor,
                ..Pass::default()
            });
        };
        let end = TrunkSeq(head.get().min(cursor.get() + limit as u64));
        let mut out = Pass::default();
        for (seq, version) in repo.trunk_range(cursor, end).await? {
            if let Err(e) = self.derive(version).await {
                let mut attempts = self.attempts.lock().await;
                let n = attempts.entry(version).or_insert(0);
                *n += 1;
                if *n < MAX_ATTEMPTS {
                    tracing::debug!(%version, seq = seq.get(), attempt = *n, error = %e, "background derivation failed; will retry");
                    break;
                }
                attempts.remove(&version);
                tracing::error!(%version, seq = seq.get(), error = %e, "background derivation failed repeatedly; skipping");
                out.abandoned += 1;
            }
            cursor = seq;
            out.versions += 1;
            self.store_cursor(cursor).await?;
        }
        out.cursor = cursor;
        Ok(out)
    }

    async fn derive(&self, version: ChangeVersionHash) -> Result<()> {
        for kind in scheduled(Schedule::Background) {
            kind.compute(&self.manager, version).await?;
        }
        Ok(())
    }
}
