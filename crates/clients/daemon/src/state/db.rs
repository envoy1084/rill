//! The daemon's state on [`redb`]: mounts, overlay entries and the stat
//! cache of eager mounts. Every change is one durable transaction, so an
//! edit acknowledged to a writer survives a crash. Blocking; call it off
//! the async runtime.

use std::path::Path;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use rill_types::{FileKind, ObjectHash, RepoPathBuf};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::mount::MountRecord;
use crate::overlay::OverlayEntry;

const MOUNTS: TableDefinition<&str, &[u8]> = TableDefinition::new("mounts");
const OVERLAY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("overlay");
const FILES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("files");

/// What an eager mount last saw of a file on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStat {
    /// Byte length.
    pub size: u64,
    /// Modification time, nanoseconds since the epoch.
    pub mtime_ns: i64,
    /// Digest of the bytes.
    pub digest: ObjectHash,
    /// File kind.
    pub kind: FileKind,
}

/// One transaction's worth of changes to a mount.
#[derive(Debug, Default)]
pub struct Batch {
    /// The mount's new record.
    pub mount: Option<MountRecord>,
    /// Overlay entries to write (`None` removes).
    pub overlay: Vec<(RepoPathBuf, Option<OverlayEntry>)>,
    /// Stat entries to write (`None` removes).
    pub files: Vec<(RepoPathBuf, Option<FileStat>)>,
}

impl Batch {
    /// True when there is nothing to write.
    pub fn is_empty(&self) -> bool {
        self.mount.is_none() && self.overlay.is_empty() && self.files.is_empty()
    }
}

/// The state database.
#[derive(Debug)]
pub struct StateDb {
    db: Database,
}

fn key(mount: &str, path: &str) -> Vec<u8> {
    let mut k = Vec::with_capacity(mount.len() + 1 + path.len());
    k.extend_from_slice(mount.as_bytes());
    k.push(0);
    k.extend_from_slice(path.as_bytes());
    k
}

fn bounds(mount: &str) -> (Vec<u8>, Vec<u8>) {
    let mut start = mount.as_bytes().to_vec();
    start.push(0);
    let mut end = mount.as_bytes().to_vec();
    end.push(1);
    (start, end)
}

impl StateDb {
    /// Open (or create) the database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let db = Database::create(path).map_err(Error::state)?;
        let tx = db.begin_write().map_err(Error::state)?;
        tx.open_table(MOUNTS).map_err(Error::state)?;
        tx.open_table(OVERLAY).map_err(Error::state)?;
        tx.open_table(FILES).map_err(Error::state)?;
        tx.commit().map_err(Error::state)?;
        Ok(StateDb { db })
    }

    /// Every mount.
    pub fn mounts(&self) -> Result<Vec<MountRecord>> {
        let tx = self.db.begin_read().map_err(Error::state)?;
        let table = tx.open_table(MOUNTS).map_err(Error::state)?;
        let mut out = Vec::new();
        for row in table.iter().map_err(Error::state)? {
            let (_, value) = row.map_err(Error::state)?;
            out.push(postcard::from_bytes(value.value())?);
        }
        Ok(out)
    }

    /// A mount's overlay entries.
    pub fn overlay(&self, mount: &str) -> Result<Vec<(RepoPathBuf, OverlayEntry)>> {
        self.scan(OVERLAY, mount)
    }

    /// A mount's stat cache.
    pub fn files(&self, mount: &str) -> Result<Vec<(RepoPathBuf, FileStat)>> {
        self.scan(FILES, mount)
    }

    /// One stat entry.
    pub fn file(&self, mount: &str, path: &RepoPathBuf) -> Result<Option<FileStat>> {
        let tx = self.db.begin_read().map_err(Error::state)?;
        let table = tx.open_table(FILES).map_err(Error::state)?;
        let row = table
            .get(key(mount, path.as_str()).as_slice())
            .map_err(Error::state)?;
        Ok(match row {
            Some(v) => Some(postcard::from_bytes(v.value())?),
            None => None,
        })
    }

    fn scan<T: serde::de::DeserializeOwned>(
        &self,
        def: TableDefinition<&[u8], &[u8]>,
        mount: &str,
    ) -> Result<Vec<(RepoPathBuf, T)>> {
        let tx = self.db.begin_read().map_err(Error::state)?;
        let table = tx.open_table(def).map_err(Error::state)?;
        let (start, end) = bounds(mount);
        let mut out = Vec::new();
        for row in table
            .range(start.as_slice()..end.as_slice())
            .map_err(Error::state)?
        {
            let (k, v) = row.map_err(Error::state)?;
            let path = std::str::from_utf8(&k.value()[start.len()..])
                .map_err(|e| Error::State(format!("stored path is not UTF-8: {e}")))?;
            out.push((RepoPathBuf::new(path)?, postcard::from_bytes(v.value())?));
        }
        Ok(out)
    }

    /// Apply a batch to mount `mount` in one durable transaction.
    pub fn apply(&self, mount: &str, batch: &Batch) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        let tx = self.db.begin_write().map_err(Error::state)?;
        {
            if let Some(record) = &batch.mount {
                let mut t = tx.open_table(MOUNTS).map_err(Error::state)?;
                t.insert(mount, postcard::to_stdvec(record)?.as_slice())
                    .map_err(Error::state)?;
            }
            let mut t = tx.open_table(OVERLAY).map_err(Error::state)?;
            for (path, entry) in &batch.overlay {
                let k = key(mount, path.as_str());
                match entry {
                    Some(e) => t
                        .insert(k.as_slice(), postcard::to_stdvec(e)?.as_slice())
                        .map(drop),
                    None => t.remove(k.as_slice()).map(drop),
                }
                .map_err(Error::state)?;
            }
            let mut t = tx.open_table(FILES).map_err(Error::state)?;
            for (path, stat) in &batch.files {
                let k = key(mount, path.as_str());
                match stat {
                    Some(s) => t
                        .insert(k.as_slice(), postcard::to_stdvec(s)?.as_slice())
                        .map(drop),
                    None => t.remove(k.as_slice()).map(drop),
                }
                .map_err(Error::state)?;
            }
        }
        tx.commit().map_err(Error::state)
    }

    /// Forget a mount and everything recorded for it.
    pub fn delete_mount(&self, mount: &str) -> Result<()> {
        let tx = self.db.begin_write().map_err(Error::state)?;
        {
            tx.open_table(MOUNTS)
                .map_err(Error::state)?
                .remove(mount)
                .map_err(Error::state)?;
            let (start, end) = bounds(mount);
            for def in [OVERLAY, FILES] {
                tx.open_table(def)
                    .map_err(Error::state)?
                    .retain_in(start.as_slice()..end.as_slice(), |_, _| false)
                    .map_err(Error::state)?;
            }
        }
        tx.commit().map_err(Error::state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mount::MountMode;
    use crate::overlay::OverlayOp;
    use rill_types::{ObjectKind, SparseProfile, Tree};

    fn record(id: &str) -> MountRecord {
        MountRecord {
            id: id.into(),
            path: "/tmp/x".into(),
            endpoint: "http://e".into(),
            repo: "r".into(),
            mode: MountMode::Eager,
            workspace: None,
            base: "trunk".into(),
            base_version: None,
            base_tree: Tree::empty_hash(),
            working_change: None,
            sparse: SparseProfile::default(),
            etag: None,
            synced: 0,
        }
    }

    #[test]
    fn mounts_keep_their_own_rows_and_deletion_removes_only_theirs() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.redb")).unwrap();
        let entry = OverlayEntry {
            op: OverlayOp::Delete,
            seq: 1,
            conflict: false,
        };
        let stat = FileStat {
            size: 1,
            mtime_ns: 2,
            digest: ObjectHash::digest(ObjectKind::Blob, b"x"),
            kind: FileKind::Regular,
        };
        let p = RepoPathBuf::new("a/b").unwrap();
        for id in ["m1", "m10"] {
            db.apply(
                id,
                &Batch {
                    mount: Some(record(id)),
                    overlay: vec![(p.clone(), Some(entry))],
                    files: vec![(p.clone(), Some(stat))],
                },
            )
            .unwrap();
        }
        db.delete_mount("m1").unwrap();
        drop(db);
        let db = StateDb::open(&dir.path().join("state.redb")).unwrap();
        assert_eq!(db.mounts().unwrap(), vec![record("m10")]);
        assert!(db.overlay("m1").unwrap().is_empty());
        assert_eq!(db.overlay("m10").unwrap(), vec![(p.clone(), entry)]);
        assert_eq!(db.file("m10", &p).unwrap(), Some(stat));
    }
}
