//! Land requests, kept in the store so any server can accept one and
//! report its status, and a lander that takes over finds every request
//! the last one had not finished.
//!
//! A request is a record per ticket, a row in the pending index while it
//! is unfinished (ordered by lane, then age), and an index from its change
//! to the ticket, so a change has at most one unfinished request. Every
//! status change writes its event; a terminal status drops both indexes in
//! the same transaction.

use std::time::Duration;

use bytes::Bytes;
use rill_types::{
    ids::random_id, ChangeId, EventKind, LandRequest, LandRequestRecord, LandStatus, LandTicket,
    Principal, Timestamp,
};

use super::codec::{decode_record, encode_record, parse_seq};
use super::leases::Lease;
use super::store::RepoStore;
use crate::error::{Error, Result};
use crate::keys::table;
use crate::meta::{KeyRange, Mutation, Transaction};

impl RepoStore {
    /// Queue `request` and return its ticket, and whether it is new: a
    /// change that already has an unfinished request gets that ticket.
    pub async fn queue_land(&self, request: &LandRequest) -> Result<(LandTicket, bool)> {
        if let Some(ticket) = self.active_land_request(&request.change_id).await? {
            return Ok((ticket, false));
        }
        let keys = self.keys();
        let ticket = LandTicket(u64::from_be_bytes(
            random_id()[..8]
                .try_into()
                .expect("16 random bytes split into 8"),
        ));
        let record = LandRequestRecord {
            ticket,
            request: request.clone(),
            status: LandStatus::Queued,
            updated_at: Timestamp::now(),
        };
        let by_change = keys.land_by_change(&request.change_id);
        let record_key = keys.land_request(ticket.0);
        let mut tx = Transaction::new()
            .expect_absent(by_change.clone())
            .expect_absent(record_key.clone())
            .set(record_key, encode_record(&record)?)
            .set(self.pending_key(&record), Bytes::new())
            .set(by_change, Bytes::copy_from_slice(&ticket.0.to_be_bytes()));
        tx.mutations.extend(self.event_by(
            &request.requester,
            EventKind::LandRequested {
                ticket,
                change: request.change_id,
                lane: request.lane,
            },
        )?);
        tx.mutations.push(Mutation::Add {
            key: keys.table(table::LAND_WAKE),
            delta: 1,
        });
        match self.meta().transact(tx).await {
            Ok(_) => Ok((ticket, true)),
            Err(Error::Conflict(_)) => match self.active_land_request(&request.change_id).await? {
                Some(ticket) => Ok((ticket, false)),
                None => Err(Error::Conflict("land request raced".into())),
            },
            Err(e) => Err(e),
        }
    }

    /// The request with `ticket`, finished or not.
    pub async fn land_request(&self, ticket: LandTicket) -> Result<Option<LandRequestRecord>> {
        match self.meta().get(&self.keys().land_request(ticket.0)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    /// The ticket of `change`'s unfinished request, if any.
    pub async fn active_land_request(&self, change: &ChangeId) -> Result<Option<LandTicket>> {
        match self.meta().get(&self.keys().land_by_change(change)).await? {
            Some(bytes) => Ok(Some(LandTicket(parse_seq(&bytes)?))),
            None => Ok(None),
        }
    }

    /// Up to `limit` unfinished requests, most urgent lane first, oldest
    /// first within a lane.
    pub async fn pending_land_requests(&self, limit: usize) -> Result<Vec<LandRequestRecord>> {
        let prefix = self.keys().land_pending_all();
        let rows = self
            .meta()
            .scan(KeyRange::prefix(prefix.clone()).limit(limit))
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for (key, _) in rows {
            let (_, _, ticket): (i64, i64, i64) = crate::keys::tail(&prefix, &key)?;
            if let Some(record) = self.land_request(LandTicket(ticket as u64)).await? {
                out.push(record);
            }
        }
        Ok(out)
    }

    /// The transaction moving `record` to `status`, by `actor`, for a
    /// larger transaction (the landing itself, for `Landed`).
    pub fn land_status_tx(
        &self,
        record: &LandRequestRecord,
        status: LandStatus,
        actor: &Principal,
    ) -> Result<Transaction> {
        let keys = self.keys();
        let next = LandRequestRecord {
            status: status.clone(),
            updated_at: Timestamp::now().max(record.updated_at),
            ..record.clone()
        };
        let mut tx =
            Transaction::new().set(keys.land_request(record.ticket.0), encode_record(&next)?);
        if status.is_terminal() {
            tx = tx
                .delete(self.pending_key(record))
                .delete(keys.land_by_change(&record.request.change_id));
        }
        tx.mutations.extend(self.event_by(
            actor,
            EventKind::LandStatusChanged {
                ticket: record.ticket,
                change: record.request.change_id,
                status,
            },
        )?);
        Ok(tx)
    }

    /// Move the request with `ticket` to `status`, fenced by `fence` when
    /// given. A finished request never changes again.
    pub async fn set_land_status(
        &self,
        ticket: LandTicket,
        status: LandStatus,
        fence: Option<&Lease>,
        actor: &Principal,
    ) -> Result<LandRequestRecord> {
        let key = self.keys().land_request(ticket.0);
        let bytes = self
            .meta()
            .get(&key)
            .await?
            .ok_or_else(|| Error::NotFound(format!("land request {ticket}")))?;
        let record: LandRequestRecord = decode_record(&bytes)?;
        if record.status.is_terminal() {
            return Err(Error::Conflict(format!(
                "land request {ticket} already finished as {}",
                record.status.name()
            )));
        }
        let mut tx = self
            .land_status_tx(&record, status.clone(), actor)?
            .expect(key, Some(bytes));
        if let Some(lease) = fence {
            tx.conditions.push(lease.fence(self));
        }
        self.meta().transact(tx).await?;
        Ok(LandRequestRecord { status, ..record })
    }

    /// Requests queued so far, to pass to [`RepoStore::wait_for_land_requests`].
    pub async fn land_wake_count(&self) -> Result<i64> {
        self.meta()
            .counter(&self.keys().table(table::LAND_WAKE))
            .await
    }

    /// Wait until a request is queued after the count was `seen`, or
    /// `timeout` passes.
    pub async fn wait_for_land_requests(&self, seen: i64, timeout: Duration) -> Result<()> {
        self.meta()
            .wait_counter(&self.keys().table(table::LAND_WAKE), seen, timeout)
            .await?;
        Ok(())
    }

    /// Delete up to `limit` finished requests last updated before
    /// `before`; returns how many.
    pub async fn sweep_land_requests(&self, before: Timestamp, limit: usize) -> Result<usize> {
        let rows = self
            .meta()
            .scan(KeyRange::prefix(self.keys().land_requests()))
            .await?;
        let mut tx = Transaction::new();
        let mut removed = 0;
        for (key, value) in rows {
            if removed == limit {
                break;
            }
            let record: LandRequestRecord = decode_record(&value)?;
            if record.status.is_terminal() && record.updated_at < before {
                tx = tx.expect(key.clone(), Some(value)).delete(key);
                removed += 1;
            }
        }
        if removed > 0 {
            self.meta().transact(tx).await?;
        }
        Ok(removed)
    }

    fn pending_key(&self, record: &LandRequestRecord) -> Bytes {
        self.keys().land_pending(
            record.request.lane.rank(),
            record.request.requested_at.millis(),
            record.ticket.0,
        )
    }
}
