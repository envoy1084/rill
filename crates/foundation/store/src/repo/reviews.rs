//! Reviews stored as parts: one change record, one row per vote, a
//! header per thread and one stamped row per comment.
//!
//! Writers read a [`ReviewSnapshot`], change the assembled
//! [`ReviewState`], and [`RepoStore::review_writes`] turns the difference
//! into the rows that changed. Every write is checked against the change
//! record it read, so a write based on stale status or versions retries.
//! Votes and comments are blind writes: reviewers never conflict with each
//! other. A write that changes the record also checks the vote epoch, a
//! row every vote rewrites, so a status computed from the votes it saw
//! cannot overwrite one computed from more.

use std::collections::BTreeMap;

use bytes::Bytes;
use rill_types::{
    ChangeId, ChangeRecord, Comment, Event, EventKind, LabelVote, Principal, ReviewState, Thread,
    ThreadHeader, Timestamp,
};

use super::codec::{decode_record, encode_record};
use super::store::RepoStore;
use crate::error::{Error, Result};
use crate::keys::table;
use crate::meta::{KeyRange, Transaction};

/// A review as read, with what a later write must check against.
#[derive(Debug, Clone)]
pub struct ReviewSnapshot {
    pub state: ReviewState,
    record_row: Bytes,
    epoch: Option<Bytes>,
    comments: BTreeMap<String, usize>,
}

type VoteKey = (String, String, rill_types::ChangeVersionHash);

fn vote_key(v: &LabelVote) -> VoteKey {
    (v.label.clone(), v.voter.id.to_string(), v.version)
}

impl RepoStore {
    /// The assembled review of a change.
    pub async fn get_review(&self, id: &ChangeId) -> Result<Option<ReviewState>> {
        Ok(self.read_review(id).await?.map(|s| s.state))
    }

    /// The review of a change with what a write must check against.
    pub async fn read_review(&self, id: &ChangeId) -> Result<Option<ReviewSnapshot>> {
        let keys = self.keys();
        let Some(record_row) = self.meta().get(&keys.change(id)).await? else {
            return Ok(None);
        };
        let record: ChangeRecord = decode_record(&record_row)?;
        let epoch = self.meta().get(&keys.vote_epoch(id)).await?;
        let mut votes = self
            .meta()
            .scan(KeyRange::prefix(keys.votes(id)))
            .await?
            .iter()
            .map(|(_, v)| decode_record::<LabelVote>(v))
            .collect::<Result<Vec<_>>>()?;
        votes.sort_by_key(|v| v.voted_at);
        let mut threads = Vec::new();
        let mut comments = BTreeMap::new();
        for (_, row) in self.meta().scan(KeyRange::prefix(keys.threads(id))).await? {
            let header: ThreadHeader = decode_record(&row)?;
            let body = self
                .meta()
                .scan(KeyRange::prefix(keys.comments(id, &header.id)))
                .await?
                .iter()
                .map(|(_, v)| decode_record::<Comment>(v))
                .collect::<Result<Vec<_>>>()?;
            comments.insert(header.id.clone(), body.len());
            threads.push(Thread::assemble(header, body));
        }
        Ok(Some(ReviewSnapshot {
            state: ReviewState::assemble(record, votes, threads),
            record_row,
            epoch,
            comments,
        }))
    }

    /// Store a new review; fails with a conflict if the change has one.
    pub async fn create_review(&self, state: &ReviewState, actor: &Principal) -> Result<()> {
        let key = self.keys().change(&state.change_id);
        let mut tx = Transaction::new()
            .expect_absent(key.clone())
            .set(key, encode_record(&state.record())?);
        if let Some(version) = state.current_version() {
            tx.mutations.extend(self.event(&Event {
                kind: EventKind::ReviewSubmitted {
                    change: state.change_id,
                    version,
                },
                actor: actor.clone(),
                at: state.created_at,
            })?);
        }
        for vote in &state.votes {
            tx = self.put_vote(tx, &state.change_id, vote)?;
        }
        for thread in &state.threads {
            tx = self.put_thread(tx, &state.change_id, thread)?;
        }
        self.meta().transact(tx).await?;
        Ok(())
    }

    /// The transaction turning `prev` into `next`, to commit alone with
    /// [`RepoStore::write_review`] or as part of a larger write. It records
    /// one event per change it makes: votes, comments, thread resolutions,
    /// stack and status changes. `actor` is who caused the record and
    /// thread changes; votes and comments carry their own authors.
    pub fn review_writes(
        &self,
        prev: &ReviewSnapshot,
        next: &ReviewState,
        actor: &Principal,
    ) -> Result<Transaction> {
        let id = &next.change_id;
        if *id != prev.state.change_id {
            return Err(Error::Invalid("review write for another change".into()));
        }
        let keys = self.keys();
        let mut tx = Transaction::new().expect(keys.change(id), Some(prev.record_row.clone()));
        let now = Timestamp::now();
        let mut events: Vec<Event> = Vec::new();
        let mut emit = |kind, by: &Principal| {
            events.push(Event {
                kind,
                actor: by.clone(),
                at: now,
            })
        };

        let before: BTreeMap<VoteKey, &LabelVote> =
            prev.state.votes.iter().map(|v| (vote_key(v), v)).collect();
        let after: BTreeMap<VoteKey, &LabelVote> =
            next.votes.iter().map(|v| (vote_key(v), v)).collect();
        let mut votes_changed = false;
        for (key, vote) in &after {
            if before.get(key) != Some(vote) {
                tx = self.put_vote(tx, id, vote)?;
                votes_changed = true;
                emit(
                    EventKind::Voted {
                        change: *id,
                        label: vote.label.clone(),
                        value: vote.value,
                        version: vote.version,
                    },
                    &vote.voter,
                );
            }
        }
        for (label, voter, version) in before.keys().filter(|k| !after.contains_key(*k)) {
            tx = tx.delete(keys.vote(id, label, voter, version));
            votes_changed = true;
        }
        if votes_changed {
            tx = tx.set(
                keys.vote_epoch(id),
                Bytes::copy_from_slice(&rill_types::ids::random_id()),
            );
        }

        let record = next.record();
        let old_record = prev.state.record();
        if record != old_record {
            tx = tx
                .expect(keys.vote_epoch(id), prev.epoch.clone())
                .set(keys.change(id), encode_record(&record)?);
            if record.stack != old_record.stack {
                emit(
                    EventKind::StackSet {
                        change: *id,
                        stack: record.stack.clone(),
                    },
                    actor,
                );
            }
            if record.status != old_record.status {
                emit(
                    EventKind::StatusChanged {
                        change: *id,
                        from: old_record.status,
                        to: record.status,
                    },
                    actor,
                );
            }
        }

        let old_threads: BTreeMap<&str, &Thread> = prev
            .state
            .threads
            .iter()
            .map(|t| (t.id.as_str(), t))
            .collect();
        for thread in &next.threads {
            match old_threads.get(thread.id.as_str()) {
                None => {
                    tx = tx.expect_absent(keys.thread(id, &thread.id));
                    tx = self.put_thread(tx, id, thread)?;
                    for (i, comment) in thread.comments.iter().enumerate() {
                        emit(
                            EventKind::Commented {
                                change: *id,
                                thread: thread.id.clone(),
                                new_thread: i == 0,
                            },
                            &comment.author,
                        );
                    }
                }
                Some(old) => {
                    let stored = prev.comments.get(&thread.id).copied().unwrap_or(0);
                    if thread.comments.len() < stored
                        || thread.comments[..stored] != old.comments[..stored]
                    {
                        return Err(Error::Invalid(format!(
                            "thread {} would lose or rewrite comments",
                            thread.id
                        )));
                    }
                    if thread.header() != old.header() {
                        tx = tx.set(
                            keys.thread(id, &thread.id),
                            encode_record(&thread.header())?,
                        );
                        if thread.resolved != old.resolved {
                            emit(
                                EventKind::ThreadResolved {
                                    change: *id,
                                    thread: thread.id.clone(),
                                    resolved: thread.resolved,
                                },
                                actor,
                            );
                        }
                    }
                    for comment in &thread.comments[stored..] {
                        tx = tx
                            .append_stamped(keys.comments(id, &thread.id), encode_record(comment)?);
                        emit(
                            EventKind::Commented {
                                change: *id,
                                thread: thread.id.clone(),
                                new_thread: false,
                            },
                            &comment.author,
                        );
                    }
                }
            }
        }
        for event in &events {
            tx.mutations.extend(self.event(event)?);
        }
        Ok(tx)
    }

    /// Write `next` over `prev`; a conflict means another writer changed
    /// what `prev` was read from.
    pub async fn write_review(
        &self,
        prev: &ReviewSnapshot,
        next: &ReviewState,
        actor: &Principal,
    ) -> Result<()> {
        let tx = self.review_writes(prev, next, actor)?;
        self.meta().transact(tx).await?;
        Ok(())
    }

    /// Every change record in the repository, without votes or threads.
    pub async fn list_change_records(&self) -> Result<Vec<ChangeRecord>> {
        self.list_records(table::CHANGE).await
    }

    /// Every review in the repository, assembled.
    pub async fn list_reviews(&self) -> Result<Vec<ReviewState>> {
        let mut out = Vec::new();
        for record in self.list_change_records().await? {
            if let Some(state) = self.get_review(&record.change_id).await? {
                out.push(state);
            }
        }
        Ok(out)
    }

    fn put_vote(&self, tx: Transaction, id: &ChangeId, vote: &LabelVote) -> Result<Transaction> {
        let key = self
            .keys()
            .vote(id, &vote.label, &vote.voter.id.to_string(), &vote.version);
        Ok(tx.set(key, encode_record(vote)?))
    }

    fn put_thread(
        &self,
        mut tx: Transaction,
        id: &ChangeId,
        thread: &Thread,
    ) -> Result<Transaction> {
        let keys = self.keys();
        tx = tx.set(
            keys.thread(id, &thread.id),
            encode_record(&thread.header())?,
        );
        for comment in &thread.comments {
            tx = tx.append_stamped(keys.comments(id, &thread.id), encode_record(comment)?);
        }
        Ok(tx)
    }
}
