//! Indexing a root: its path layer (a delta from its parent when it can),
//! and segments for the blobs no segment holds yet.

use std::collections::HashSet;
use std::sync::Arc;

use futures::stream::{self, StreamExt};
use rill_core::{diff_trees, is_binary, read_content, RootTree};
use rill_store::repo::Lease;
use rill_types::{
    ChangeVersionHash, ContentRef, ObjectHash, ObjectKind, RepoPathBuf, SparseProfile, TreeHash,
};

use super::records::{self, SegmentRef, Through};
use super::search_index::SearchIndex;
use crate::error::Result;
use crate::paths::{PathEntry, PathLayer, PathView, MAX_DELTA_DEPTH};
use crate::segment::{PreparedDoc, Segment, SegmentBuilder, SegmentBytes};
use crate::symbols::extract_symbols;

/// Changed files above which a new base is written instead of a delta.
const MAX_DELTA_CHANGES: usize = 50_000;

/// What indexing a root did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexStats {
    pub files: usize,
    pub blobs_indexed: usize,
    pub blobs_skipped: usize,
    pub segments_written: usize,
    /// The root's layer was a delta from its parent.
    pub delta: bool,
    pub already_indexed: bool,
}

/// A blob to index: its content and a path that names its language.
struct Todo {
    path: RepoPathBuf,
    content: ContentRef,
}

impl SearchIndex {
    /// Index the root of `version`, as a delta from its first parent's
    /// root when that is indexed, recording `version`'s trunk position as
    /// indexed when it has one.
    pub async fn index_version(
        &self,
        version: &ChangeVersionHash,
        fence: Option<&Lease>,
    ) -> Result<(TreeHash, IndexStats)> {
        let root = self.derive.get::<RootTree>(version).await?;
        let cv = self.derive.change(version).await?;
        let parent = match cv.first_parent() {
            Some(p) => Some(self.derive.get::<RootTree>(&p).await?),
            None => None,
        };
        let seq = self.repo.landing_of(version).await?.map(|l| l.seq);
        let stats = self.index_root_from(root, parent, fence).await?;
        if let Some(seq) = seq {
            self.mark_through(Through { seq, root }, fence).await?;
        }
        Ok((root, stats))
    }

    /// Index `root` with a full walk (or nothing, when it is indexed).
    pub async fn index_root(&self, root: TreeHash, fence: Option<&Lease>) -> Result<IndexStats> {
        self.index_root_from(root, None, fence).await
    }

    /// Index `root`, as a delta from `parent` when `parent` is indexed and
    /// the chain is short enough.
    pub async fn index_root_from(
        &self,
        root: TreeHash,
        parent: Option<TreeHash>,
        fence: Option<&Lease>,
    ) -> Result<IndexStats> {
        let _guard = self.writing.lock().await;
        if let Some(view) = self.view(root).await? {
            return Ok(IndexStats {
                files: view.len(),
                already_indexed: true,
                ..IndexStats::default()
            });
        }
        let parent_view = match parent {
            Some(p) => self
                .view(p)
                .await?
                .filter(|v| v.depth < MAX_DELTA_DEPTH)
                .map(|v| (p, v)),
            None => None,
        };
        let mut stats = IndexStats::default();
        let (layer, view, todo) = match parent_view {
            Some((p, pv)) => {
                let entries = diff_trees(self.trees(), Some(p), Some(root)).await?;
                if entries.len() > MAX_DELTA_CHANGES {
                    self.full(root).await?
                } else {
                    stats.delta = true;
                    let changes: Vec<_> = entries
                        .iter()
                        .map(|e| {
                            (
                                e.path.clone(),
                                e.after.as_ref().map(|(c, k)| (c.content_hash(), *k)),
                            )
                        })
                        .collect();
                    let todo = entries
                        .into_iter()
                        .filter_map(|e| {
                            e.after.map(|(content, _)| Todo {
                                path: e.path,
                                content,
                            })
                        })
                        .collect();
                    let view = pv.apply(&changes);
                    let depth = view.depth;
                    (
                        PathLayer::Delta {
                            parent: p,
                            depth,
                            changes,
                        },
                        view,
                        todo,
                    )
                }
            }
            None => self.full(root).await?,
        };
        stats.files = view.len();

        let written = self.index_blobs(todo, &mut stats).await?;
        self.commit(written.refs, &written.markers, root, &layer, fence)
            .await?;
        self.views.insert(root, Arc::new(view));
        tracing::info!(root = %root, ?stats, "indexed root");
        Ok(stats)
    }

    async fn full(&self, root: TreeHash) -> Result<(PathLayer, PathView, Vec<Todo>)> {
        let entries = self
            .trees()
            .walk(root, &RepoPathBuf::root(), &SparseProfile::full())
            .await?;
        let paths: Vec<PathEntry> = entries
            .iter()
            .map(|e| PathEntry {
                path: e.path.clone(),
                hash: e.content.content_hash(),
                kind: e.kind,
            })
            .collect();
        let todo = entries
            .into_iter()
            .map(|e| Todo {
                path: e.path,
                content: e.content,
            })
            .collect();
        let view = PathView::base(paths.clone());
        Ok((PathLayer::Base(paths), view, todo))
    }

    /// Read, tokenize and write into segments every blob of `todo` no
    /// segment holds yet.
    async fn index_blobs(&self, todo: Vec<Todo>, stats: &mut IndexStats) -> Result<Written> {
        let mut unique = HashSet::new();
        let mut fresh = Vec::new();
        for t in todo {
            let hash = t.content.content_hash();
            if unique.insert(hash) && records::marker(&self.repo, &hash).await?.is_none() {
                fresh.push(t);
            }
        }
        let shards = self.config.shards.max(1) as usize;
        let mut builders: Vec<SegmentBuilder> =
            (0..shards).map(|_| SegmentBuilder::new()).collect();
        let mut written = Written::default();
        let objects = self.repo.objects().clone();
        let max = self.config.max_blob_size;
        let mut prepared = stream::iter(fresh)
            .map(|t| {
                let objects = objects.clone();
                async move {
                    let hash = t.content.content_hash();
                    // A redacted blob is indexed as empty: it matches nothing.
                    let bytes = match read_content(objects.as_ref(), &t.content).await {
                        Err(e) if e.is_redacted() => bytes::Bytes::new(),
                        other => other?,
                    };
                    if bytes.len() > max || is_binary(&bytes) {
                        return Ok::<_, crate::Error>((hash, None));
                    }
                    let doc = tokio::task::spawn_blocking(move || {
                        let symbols = extract_symbols(t.path.as_str(), &bytes);
                        PreparedDoc::new(hash, &bytes, &symbols)
                    })
                    .await??;
                    Ok((hash, Some(doc)))
                }
            })
            .buffer_unordered(self.config.concurrency.max(1));
        while let Some(item) = prepared.next().await {
            let (hash, doc) = item?;
            match doc {
                None => {
                    stats.blobs_skipped += 1;
                    written.markers.push((hash, false));
                }
                Some(doc) => {
                    stats.blobs_indexed += 1;
                    written.markers.push((hash, true));
                    let shard = self.shard_of(&hash);
                    builders[shard].push(doc);
                    if builders[shard].raw_bytes() >= self.config.segment_bytes {
                        let full = std::mem::take(&mut builders[shard]);
                        written
                            .refs
                            .push((shard, self.write_segment(full, 0).await?));
                        stats.segments_written += 1;
                    }
                }
            }
        }
        for (shard, b) in builders.into_iter().enumerate() {
            if !b.is_empty() {
                written.refs.push((shard, self.write_segment(b, 0).await?));
                stats.segments_written += 1;
            }
        }
        Ok(written)
    }

    /// Build and store one segment.
    pub(super) async fn write_segment(
        &self,
        builder: SegmentBuilder,
        level: u8,
    ) -> Result<SegmentRef> {
        let docs = builder.len() as u32;
        let bytes = tokio::task::spawn_blocking(move || builder.build()).await??;
        self.store_segment(bytes, docs, level).await
    }

    pub(super) async fn store_segment(
        &self,
        bytes: bytes::Bytes,
        docs: u32,
        level: u8,
    ) -> Result<SegmentRef> {
        let hash = ObjectHash::digest(ObjectKind::Derived, &bytes);
        self.repo
            .objects()
            .put(ObjectKind::Derived, &hash, bytes.clone())
            .await?;
        let r = SegmentRef {
            hash,
            docs,
            bytes: bytes.len() as u64,
            level,
        };
        let seg = tokio::task::spawn_blocking(move || Segment::open(SegmentBytes::Owned(bytes)))
            .await??;
        self.segments.insert(hash, Arc::new(seg));
        Ok(r)
    }

    /// Publish new segments, then markers, then the root's layer: a reader
    /// never sees a layer whose blobs are not in a listed segment.
    async fn commit(
        &self,
        refs: Vec<(usize, SegmentRef)>,
        markers: &[(ObjectHash, bool)],
        root: TreeHash,
        layer: &PathLayer,
        fence: Option<&Lease>,
    ) -> Result<()> {
        if !refs.is_empty() {
            let mut set = self.set.lock().clone();
            let shards = self.config.shards.max(1) as usize;
            if set.shards.len() < shards {
                set.shards.resize(shards, Vec::new());
            }
            for (shard, r) in refs {
                set.shards[shard].push(r);
            }
            set.generation += 1;
            records::put_segment_set(&self.repo, &set, fence).await?;
            self.install(set).await?;
        }
        for (hash, indexed) in markers {
            records::put_marker(&self.repo, hash, *indexed, fence).await?;
        }
        records::put_layer(&self.repo, &root, layer, fence).await
    }

    /// Record that trunk is indexed through `through`.
    pub async fn mark_through(&self, through: Through, fence: Option<&Lease>) -> Result<()> {
        let mut set = self.set.lock().clone();
        if set.through.is_some_and(|t| t.seq >= through.seq) {
            return Ok(());
        }
        set.through = Some(through);
        set.generation += 1;
        records::put_segment_set(&self.repo, &set, fence).await?;
        self.install(set).await
    }
}

#[derive(Default)]
struct Written {
    refs: Vec<(usize, SegmentRef)>,
    markers: Vec<(ObjectHash, bool)>,
}
