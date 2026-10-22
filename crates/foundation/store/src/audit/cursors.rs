//! How far each source log has been projected into the audit queue. A
//! cursor moves with compare-and-set in the transaction that queues what
//! it passed, so projecting twice never queues a record twice.

use bytes::Bytes;
use rill_types::audit::AuditRecord;
use rill_types::Versionstamp;

use super::store::AuditStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::Transaction;

fn cursor_key(source: &str) -> Bytes {
    keys::identity_row(table::AUDIT_CURSOR, source)
}

impl AuditStore {
    /// The last entry of `source` projected, and the raw value for the
    /// compare-and-set that moves it.
    pub async fn cursor(&self, source: &str) -> Result<(Option<Versionstamp>, Option<Bytes>)> {
        let raw = self.meta.get(&cursor_key(source)).await?;
        let stamp = match &raw {
            Some(b) => Some(Versionstamp(
                <[u8; 12]>::try_from(b.as_ref())
                    .map_err(|_| Error::Codec("bad audit cursor".into()))?,
            )),
            None => None,
        };
        Ok((stamp, raw))
    }

    /// Queue `records` and move `source`'s cursor from `seen` to `to`.
    /// Fails with a conflict when another projector moved it first.
    pub async fn project(
        &self,
        source: &str,
        seen: Option<Bytes>,
        to: Versionstamp,
        records: &[AuditRecord],
    ) -> Result<()> {
        let mut tx = Transaction::new()
            .expect(cursor_key(source), seen)
            .set(cursor_key(source), Bytes::copy_from_slice(to.as_bytes()));
        for record in records {
            tx.mutations.extend(self.queue(record)?);
        }
        self.meta.transact(tx).await?;
        Ok(())
    }
}
