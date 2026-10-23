//! Writes buffered per file. NFS has no open or close, so a file's
//! writes collect in a scratch copy (a view write handle) and become an
//! overlay entry once the file has been quiet for a moment, or when a
//! caller settles the mount (`status`, `commit`, unmount).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::error::Result;
use crate::vfs::View;

/// How long a file stays quiet before its writes are committed.
pub const QUIET: Duration = Duration::from_millis(500);

/// Open write handles by inode, with the time of their last write.
#[derive(Debug, Default)]
pub struct Writes {
    open: Mutex<HashMap<u64, (u64, Instant)>>,
}

impl Writes {
    /// The write handle of `ino`, opened on first use.
    pub async fn handle(&self, view: &View, ino: u64) -> Result<u64> {
        if let Some((fh, at)) = self.open.lock().get_mut(&ino) {
            *at = Instant::now();
            return Ok(*fh);
        }
        let opened = view.open(ino, true, false).await?;
        let existing = {
            let mut open = self.open.lock();
            match open.get(&ino) {
                Some((existing, _)) => Some(*existing),
                None => {
                    open.insert(ino, (opened.fh, Instant::now()));
                    None
                }
            }
        };
        match existing {
            Some(existing) => {
                view.release(opened.fh).await?;
                Ok(existing)
            }
            None => Ok(opened.fh),
        }
    }

    /// Adopt `fh`, just created for `ino`.
    pub fn adopt(&self, ino: u64, fh: u64) {
        self.open.lock().insert(ino, (fh, Instant::now()));
    }

    /// The open handle of `ino`, if it has one.
    pub fn get(&self, ino: u64) -> Option<u64> {
        self.open.lock().get(&ino).map(|(fh, _)| *fh)
    }

    /// Mark `ino` as just written.
    pub fn touch(&self, ino: u64) {
        if let Some((_, at)) = self.open.lock().get_mut(&ino) {
            *at = Instant::now();
        }
    }

    /// Commit and close the handles quiet for `quiet` (all with zero).
    pub async fn commit(&self, view: &View, quiet: Duration) -> Result<()> {
        let due: Vec<u64> = {
            let mut open = self.open.lock();
            let inos: Vec<u64> = open
                .iter()
                .filter(|(_, (_, at))| at.elapsed() >= quiet)
                .map(|(ino, _)| *ino)
                .collect();
            inos.iter()
                .filter_map(|ino| open.remove(ino))
                .map(|(fh, _)| fh)
                .collect()
        };
        for fh in due {
            view.release(fh).await?;
        }
        Ok(())
    }
}
