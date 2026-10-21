//! What synthetic agents do: mostly reads, some new versions, a few
//! reviews and landings, concentrated on a share of hot files.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use parking_lot::Mutex;
use rand::RngExt;
use rill_sdk::{ApplyOptions, Edit, EditBase, GrepQuery, LandStatus, LogStart, RepoClient, Root};
use rill_types::{ChangeId, FileKind, RepoPathBuf};

use crate::ledger::Ledger;
use crate::stats::Stats;

/// Directories and files the repository is seeded with.
pub const DIRS: usize = 20;
pub const FILES_PER_DIR: usize = 50;
const HOT_FILES: usize = 5;

/// Shared state of the synthetic workload.
pub struct Workload {
    pub repo: RepoClient,
    pub stats: Arc<Stats>,
    pub ledger: Arc<Ledger>,
    pub hot_share: f64,
    /// Every change writes a new file of its own, so no two interact.
    pub disjoint: bool,
    fresh: std::sync::atomic::AtomicU64,
    /// Changes created but not yet submitted.
    drafts: Mutex<Vec<ChangeId>>,
    /// Changes submitted and approved, ready to land.
    approved: Mutex<Vec<ChangeId>>,
}

impl Workload {
    pub fn new(
        repo: RepoClient,
        stats: Arc<Stats>,
        ledger: Arc<Ledger>,
        hot_share: f64,
        disjoint: bool,
    ) -> Self {
        Workload {
            repo,
            stats,
            ledger,
            hot_share,
            disjoint,
            fresh: std::sync::atomic::AtomicU64::new(0),
            drafts: Mutex::new(Vec::new()),
            approved: Mutex::new(Vec::new()),
        }
    }

    fn path(&self) -> RepoPathBuf {
        if self.disjoint {
            let n = self
                .fresh
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return RepoPathBuf::new(format!("fresh/f{n}.rs")).expect("generated paths are valid");
        }
        let mut rng = rand::rng();
        let (dir, file) = if rng.random_bool(self.hot_share) {
            (0, rng.random_range(0..HOT_FILES))
        } else {
            (
                rng.random_range(0..DIRS),
                rng.random_range(0..FILES_PER_DIR),
            )
        };
        RepoPathBuf::new(file_path(dir, file)).expect("generated paths are valid")
    }

    async fn timed<T>(
        &self,
        op: &'static str,
        f: impl std::future::Future<Output = rill_sdk::Result<T>>,
    ) -> Option<T> {
        let started = Instant::now();
        match f.await {
            Ok(v) => {
                self.stats.ok(op, started.elapsed());
                Some(v)
            }
            Err(e) => {
                self.stats.error_with(op, &e);
                None
            }
        }
    }

    /// One read, chosen like an agent exploring code.
    pub async fn read(&self) {
        let trunk = Root::trunk();
        let roll = rand::rng().random_range(0..100);
        match roll {
            0..50 => {
                let path = self.path();
                self.timed("read_file", self.repo.read_file(&trunk, &path))
                    .await;
            }
            50..70 => {
                let dir = RepoPathBuf::new(format!("d{}", rand::rng().random_range(0..DIRS)))
                    .expect("generated paths are valid");
                self.timed("list_dir", self.repo.list_dir(&trunk, &dir))
                    .await;
            }
            70..85 => {
                let query = GrepQuery::new(format!("value_{}", rand::rng().random_range(0..1000)));
                self.timed("grep", self.repo.grep(&trunk, &query)).await;
            }
            _ => {
                self.timed("log", self.repo.log(LogStart::Trunk, 20, None))
                    .await;
            }
        }
    }

    /// A new change editing one file at trunk.
    pub async fn new_version(&self) {
        let path = self.path();
        let body = format!(
            "pub const value_{}: u32 = 1;\n",
            rand::rng().random_range(0..1000)
        );
        let edit = Edit::Set {
            path,
            bytes: Bytes::from(body),
            kind: FileKind::Regular,
        };
        let opts = ApplyOptions::message("load: edit");
        if let Some(result) = self
            .timed(
                "create_version",
                self.repo
                    .apply_edits(EditBase::Root(Root::trunk()), vec![edit], opts),
            )
            .await
        {
            self.ledger.version(result.hash);
            self.drafts.lock().push(result.version.change_id);
        }
    }

    /// Submit a draft and approve it.
    pub async fn review(&self) {
        let Some(change) = self.drafts.lock().pop() else {
            return;
        };
        let reviews = self.repo.reviews();
        if self.timed("submit", reviews.submit(change)).await.is_none() {
            return;
        }
        if self
            .timed("vote", reviews.vote(change, "Review", 2, None, None))
            .await
            .is_some()
        {
            self.approved.lock().push(change);
        }
    }

    /// Request landing an approved change and wait for the outcome; the
    /// latency is queue-to-trunk.
    pub async fn land(&self) {
        let Some(change) = self.approved.lock().pop() else {
            return;
        };
        let started = Instant::now();
        let Some(ticket) = self
            .timed("request_land", self.repo.reviews().request_land(change))
            .await
        else {
            return;
        };
        match self
            .repo
            .stack()
            .await_land(&ticket.id, Duration::from_secs(120))
            .await
        {
            Ok(p) if p.status() == LandStatus::Landed => {
                self.stats.ok("land", started.elapsed());
                if let (Some(seq), Some(hash)) = (p.seq, p.landed_hash) {
                    self.ledger.landing(seq, hash);
                }
            }
            _ => self.stats.error("land"),
        }
    }
}

/// Path of seeded file `file` in directory `dir`.
pub fn file_path(dir: usize, file: usize) -> String {
    format!("d{dir}/f{file}.rs")
}
