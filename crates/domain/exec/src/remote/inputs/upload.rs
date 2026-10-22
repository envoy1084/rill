//! Copying a repository tree into an execution space as an REAPI input
//! root.
//!
//! Each directory's digest is remembered under the filter, its path and
//! its tree hash, so a tree that shares most directories with one already
//! uploaded costs only its changed directories.

use std::future::Future;
use std::pin::Pin;

use bytes::Bytes;
use futures::TryStreamExt;
use prost::Message;
use rill_core::{read_content, TreeOps};
use rill_reapi_proto::reapi::{Directory, DirectoryNode, FileNode, SymlinkNode};
use rill_types::{ContentRef, EntryKind, FileKind, ObjectHash, ObjectKind, RepoPathBuf, TreeHash};

use super::filter::InputFilter;
use crate::error::{Error, Result};
use crate::remote::digest::BlobDigest;
use crate::remote::space::ExecSpace;

/// Derived kind: a directory under a filter to its `Directory` digest.
const INPUT_INDEX: &str = "reapi:input";
/// Derived kind: chunked content's manifest to the digest of its bytes.
const WHOLE_INDEX: &str = "reapi:whole";
/// Copies into the space in flight at once.
const COPY_CONCURRENCY: usize = 16;

type Boxed<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

impl ExecSpace {
    /// The input root holding the files of `root` that `filter` selects,
    /// with every file and directory stored in this space.
    pub async fn input_root(
        &self,
        trees: &TreeOps,
        root: TreeHash,
        filter: &InputFilter,
    ) -> Result<BlobDigest> {
        let digest = self
            .directory(trees, RepoPathBuf::root(), root, filter)
            .await?;
        Ok(digest.unwrap_or_else(|| BlobDigest::of(&[])))
    }

    /// The digest of the directory `hash` at `path`, or `None` when the
    /// filter leaves it empty.
    fn directory<'a>(
        &'a self,
        trees: &'a TreeOps,
        path: RepoPathBuf,
        hash: TreeHash,
        filter: &'a InputFilter,
    ) -> Boxed<'a, Option<BlobDigest>> {
        Box::pin(async move {
            let key = index_key(filter, &path, &hash);
            if let Some(known) = self.records.get_derived(INPUT_INDEX, &key).await? {
                return decode_index(&known);
            }
            let tree = trees.get_tree(&hash).await?;
            let mut dir = Directory::default();
            let mut copies = Vec::new();
            for (name, entry) in &tree.entries {
                let child = path.join(name)?;
                match &entry.kind {
                    EntryKind::File { content, kind } if filter.matches(&child) => match kind {
                        FileKind::Symlink => {
                            let target = read_content(trees.objects().as_ref(), content).await?;
                            dir.symlinks.push(SymlinkNode {
                                name: name.clone(),
                                target: String::from_utf8_lossy(&target).into_owned(),
                                node_properties: None,
                            });
                        }
                        FileKind::Regular | FileKind::Executable => {
                            let digest = self.content_digest(trees, content).await?;
                            dir.files.push(FileNode {
                                name: name.clone(),
                                digest: Some(digest.to_proto()),
                                is_executable: kind.is_executable(),
                                node_properties: None,
                            });
                            copies.push((digest, content.clone()));
                        }
                    },
                    EntryKind::File { .. } => {}
                    EntryKind::Dir { hash, .. } => {
                        if !filter.may_enter(&child) {
                            continue;
                        }
                        if let Some(digest) = self.directory(trees, child, *hash, filter).await? {
                            dir.directories.push(DirectoryNode {
                                name: name.clone(),
                                digest: Some(digest.to_proto()),
                            });
                        }
                    }
                }
            }
            self.copy_missing(trees, copies).await?;
            let digest =
                if dir.files.is_empty() && dir.directories.is_empty() && dir.symlinks.is_empty() {
                    None
                } else {
                    Some(self.put(Bytes::from(dir.encode_to_vec())).await?)
                };
            self.records
                .put_derived(INPUT_INDEX, &key, encode_index(digest))
                .await?;
            Ok(digest)
        })
    }

    /// The REAPI digest of repository content: plain BLAKE3 of the bytes
    /// (ADR 016), which only chunked content has to be read to learn.
    async fn content_digest(&self, trees: &TreeOps, content: &ContentRef) -> Result<BlobDigest> {
        match content {
            ContentRef::Inline { bytes } => Ok(BlobDigest::of(bytes)),
            ContentRef::Blob { hash, size } => Ok(BlobDigest {
                hash: *hash,
                size: *size,
            }),
            ContentRef::Chunked { hash, .. } => {
                if let Some(known) = self.records.get_derived(WHOLE_INDEX, hash).await? {
                    return decode_index(&known)?
                        .ok_or_else(|| Error::Codec("empty whole-content index".into()));
                }
                let digest = self
                    .put(read_content(trees.objects().as_ref(), content).await?)
                    .await?;
                self.records
                    .put_derived(WHOLE_INDEX, hash, encode_index(Some(digest)))
                    .await?;
                Ok(digest)
            }
        }
    }

    /// Copy the files this space lacks from the repository.
    async fn copy_missing(
        &self,
        trees: &TreeOps,
        files: Vec<(BlobDigest, ContentRef)>,
    ) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        let digests: Vec<_> = files.iter().map(|(d, _)| *d).collect();
        let missing: std::collections::HashSet<_> =
            self.missing(&digests).await?.into_iter().collect();
        futures::stream::iter(
            files
                .into_iter()
                .filter(|(d, _)| missing.contains(d))
                .map(Ok::<_, Error>),
        )
        .try_for_each_concurrent(COPY_CONCURRENCY, |(digest, content)| async move {
            let bytes = read_content(trees.objects().as_ref(), &content).await?;
            self.write(&digest, bytes).await
        })
        .await
    }
}

fn index_key(filter: &InputFilter, path: &RepoPathBuf, hash: &TreeHash) -> ObjectHash {
    let mut hasher = ObjectHash::hasher(ObjectKind::Derived);
    hasher
        .update(filter.fingerprint().as_bytes())
        .update(path.as_str().as_bytes())
        .update(&[0])
        .update(hash.as_bytes());
    hasher.finalize()
}

/// A digest as 40 bytes (hash, size), or nothing for an empty directory.
fn encode_index(digest: Option<BlobDigest>) -> Bytes {
    match digest {
        None => Bytes::new(),
        Some(d) => {
            let mut out = Vec::with_capacity(40);
            out.extend_from_slice(d.hash.as_bytes());
            out.extend_from_slice(&d.size.to_be_bytes());
            Bytes::from(out)
        }
    }
}

fn decode_index(bytes: &[u8]) -> Result<Option<BlobDigest>> {
    match bytes.len() {
        0 => Ok(None),
        40 => {
            let mut size = [0u8; 8];
            size.copy_from_slice(&bytes[32..]);
            Ok(Some(BlobDigest {
                hash: ObjectHash::from_slice(&bytes[..32])?,
                size: u64::from_be_bytes(size),
            }))
        }
        n => Err(Error::Codec(format!("input index entry of {n} bytes"))),
    }
}
