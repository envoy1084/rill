//! Open segments, shared by every query: fetched from the object store
//! once, kept on local disk and memory-mapped when a cache directory is
//! configured, otherwise held in memory.

use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use rill_store::RepoStore;
use rill_types::{ObjectHash, ObjectKind};

use super::records::SegmentRef;
use crate::error::{Error, Result};
use crate::segment::{Segment, SegmentBytes};

pub struct SegmentCache {
    open: DashMap<ObjectHash, Arc<Segment>>,
    dir: Option<PathBuf>,
}

impl SegmentCache {
    pub fn new(dir: Option<PathBuf>) -> Self {
        SegmentCache {
            open: DashMap::new(),
            dir,
        }
    }

    /// The segment `r` names.
    pub async fn get(&self, repo: &RepoStore, r: &SegmentRef) -> Result<Arc<Segment>> {
        if let Some(seg) = self.open.get(&r.hash) {
            return Ok(seg.clone());
        }
        let bytes = match &self.dir {
            Some(dir) => self.mapped(repo, dir, &r.hash).await?,
            None => SegmentBytes::Owned(self.fetch(repo, &r.hash).await?),
        };
        let seg = Arc::new(tokio::task::spawn_blocking(move || Segment::open(bytes)).await??);
        self.open.insert(r.hash, seg.clone());
        Ok(seg)
    }

    /// Keep a segment just written, without fetching it back.
    pub fn insert(&self, hash: ObjectHash, seg: Arc<Segment>) {
        self.open.insert(hash, seg);
    }

    /// Forget segments no shard lists any more.
    pub fn retain(&self, live: &std::collections::HashSet<ObjectHash>) {
        self.open.retain(|h, _| live.contains(h));
        if let Some(dir) = &self.dir {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for e in entries.flatten() {
                    let name = e.file_name();
                    let keep = name
                        .to_str()
                        .and_then(|n| n.strip_suffix(".seg"))
                        .and_then(|h| ObjectHash::from_hex(h).ok())
                        .is_some_and(|h| live.contains(&h));
                    if !keep {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        }
    }

    async fn fetch(&self, repo: &RepoStore, hash: &ObjectHash) -> Result<bytes::Bytes> {
        repo.objects()
            .get(ObjectKind::Derived, hash)
            .await?
            .ok_or_else(|| Error::Codec(format!("segment {hash} is missing")))
    }

    async fn mapped(
        &self,
        repo: &RepoStore,
        dir: &std::path::Path,
        hash: &ObjectHash,
    ) -> Result<SegmentBytes> {
        let path = dir.join(format!("{}.seg", hash.to_hex()));
        if !path.exists() {
            let bytes = self.fetch(repo, hash).await?;
            let (dir, tmp, path2) = (dir.to_path_buf(), path.with_extension("tmp"), path.clone());
            tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                std::fs::create_dir_all(&dir)?;
                std::fs::write(&tmp, &bytes)?;
                std::fs::rename(&tmp, &path2)
            })
            .await?
            .map_err(|e| Error::Codec(format!("caching segment: {e}")))?;
        }
        let map = tokio::task::spawn_blocking(move || -> std::io::Result<memmap2::Mmap> {
            let file = std::fs::File::open(&path)?;
            // Safety: segment files are immutable once renamed into place,
            // and only this cache writes or removes them.
            unsafe { memmap2::Mmap::map(&file) }
        })
        .await?
        .map_err(|e| Error::Codec(format!("mapping segment: {e}")))?;
        Ok(SegmentBytes::Mapped(Arc::new(map)))
    }
}
