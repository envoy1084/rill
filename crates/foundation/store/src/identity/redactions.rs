//! Redaction requests and the objects they redact.

use rill_types::identity::{IdentityChange, Redaction};
use rill_types::{ObjectHash, Principal, Timestamp};

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

impl IdentityStore {
    /// Record a pending request.
    pub async fn request_redaction(&self, redaction: &Redaction) -> Result<()> {
        let key = keys::identity_pair(
            table::REDACTION,
            redaction.org.as_str(),
            redaction.id.as_str(),
        );
        let tx = Transaction::new()
            .expect_absent(key.clone())
            .set(key, encode_record(redaction)?);
        let tx = self.logged(
            tx,
            IdentityChange::RedactionRequested {
                org: redaction.org.clone(),
                id: redaction.id.clone(),
                hash: redaction.hash,
            },
            &redaction.requested_by,
        )?;
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// Apply request `id` of `org` on `approver`'s approval, who must not
    /// be the requester. Returns the applied redaction.
    pub async fn approve_redaction(
        &self,
        org: &str,
        id: &str,
        approver: &Principal,
    ) -> Result<Redaction> {
        let key = keys::identity_pair(table::REDACTION, org, id);
        let raw = self
            .meta
            .get(&key)
            .await?
            .ok_or_else(|| Error::NotFound(format!("redaction {id}")))?;
        let mut redaction: Redaction = decode_record(&raw)?;
        if redaction.is_applied() {
            return Err(Error::Conflict(format!(
                "redaction {id} is already applied"
            )));
        }
        if redaction.requested_by.id == approver.id {
            return Err(Error::Invalid(
                "a redaction needs a second administrator; the requester cannot approve it".into(),
            ));
        }
        redaction.approved_by = Some(approver.clone());
        redaction.applied_at = Some(Timestamp::now());
        let tx = Transaction::new()
            .expect(key.clone(), Some(raw))
            .set(key, encode_record(&redaction)?)
            .set(
                keys::identity_pair(table::REDACTED, org, redaction.hash.as_bytes().as_slice()),
                bytes::Bytes::copy_from_slice(id.as_bytes()),
            );
        let tx = self.logged(
            tx,
            IdentityChange::RedactionApplied {
                org: org.to_string(),
                id: id.to_string(),
                hash: redaction.hash,
            },
            approver,
        )?;
        self.meta.transact(tx).await?;
        Ok(redaction)
    }

    /// `org`'s requests, pending and applied.
    pub async fn redactions(&self, org: &str) -> Result<Vec<Redaction>> {
        self.meta
            .scan(KeyRange::prefix(keys::identity_row(table::REDACTION, org)))
            .await?
            .iter()
            .map(|(_, v)| decode_record(v))
            .collect()
    }

    /// Every applied redaction in the instance: `(org, hash)`.
    pub async fn redacted_objects(&self) -> Result<Vec<(String, ObjectHash)>> {
        let mut out = Vec::new();
        for (_, v) in self
            .meta
            .scan(KeyRange::prefix(keys::identity_table(table::REDACTION)))
            .await?
        {
            let r: Redaction = decode_record(&v)?;
            if r.is_applied() {
                out.push((r.org, r.hash));
            }
        }
        Ok(out)
    }
}
