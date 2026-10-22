//! The mock repository's state: trees, blobs, trunk and recorded calls.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use bytes::Bytes;
use rill_proto::convert::hash_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{FileKind, ObjectHash, ObjectKind, INLINE_THRESHOLD};
use tonic::{Request, Status};

use super::rpc::hex;

/// A file the mock serves.
#[derive(Clone, Debug)]
pub struct MockFile {
    pub bytes: Bytes,
    pub kind: FileKind,
}

impl MockFile {
    pub fn regular(bytes: impl Into<Bytes>) -> Self {
        MockFile {
            bytes: bytes.into(),
            kind: FileKind::Regular,
        }
    }
}

/// Root label → path → file.
pub type Trees = HashMap<String, BTreeMap<String, MockFile>>;

pub struct MockRepo {
    pub trees: Mutex<Trees>,
    pub blobs: Mutex<HashMap<ObjectHash, Bytes>>,
    pub log: Mutex<Vec<pb::ChangeInfo>>,
    pub trunk: Mutex<(u64, ObjectHash)>,
    pub auth_headers: Mutex<Vec<Option<String>>>,
    pub applied: Mutex<Vec<pb::ApplyEditsRequest>>,
    pub put_requests: Mutex<Vec<usize>>,
    pub rebases: Mutex<Vec<(ObjectHash, ObjectHash)>>,
    pub chunk_size: usize,
    /// `ApplyEdits` calls still to fail with `Unavailable`.
    pub unavailable: std::sync::atomic::AtomicUsize,
}

impl Default for MockRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl MockRepo {
    pub fn new() -> Self {
        MockRepo {
            trees: Mutex::default(),
            blobs: Mutex::default(),
            log: Mutex::default(),
            trunk: Mutex::new((0, ObjectHash::ZERO)),
            auth_headers: Mutex::default(),
            applied: Mutex::default(),
            put_requests: Mutex::default(),
            rebases: Mutex::default(),
            chunk_size: 1000,
            unavailable: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Install a tree under a root label (`"trunk"`, a hex hash, `"trunk@3"`).
    pub fn set_tree(&self, label: &str, files: Vec<(&str, MockFile)>) {
        let mut trees = self.trees.lock().unwrap();
        let map = files.into_iter().map(|(p, f)| (p.to_string(), f)).collect();
        trees.insert(label.to_string(), map);
    }

    pub fn set_trunk(&self, seq: u64, hash: ObjectHash) {
        *self.trunk.lock().unwrap() = (seq, hash);
    }

    pub(super) fn root_label(root: Option<&pb::RootSpec>) -> Result<String, Status> {
        match root.and_then(|r| r.root.as_ref()) {
            Some(pb::root_spec::Root::Ref(name)) => Ok(name.clone()),
            Some(pb::root_spec::Root::VersionHash(h)) => Ok(hex(h)),
            Some(pb::root_spec::Root::TreeHash(h)) => Ok(format!("tree:{}", hex(h))),
            Some(pb::root_spec::Root::TrunkSeq(n)) => Ok(format!("trunk@{n}")),
            None => Err(Status::invalid_argument("missing root")),
        }
    }

    pub(super) fn tree(
        &self,
        root: Option<&pb::RootSpec>,
    ) -> Result<BTreeMap<String, MockFile>, Status> {
        let label = Self::root_label(root)?;
        self.trees
            .lock()
            .unwrap()
            .get(&label)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("root {label}")))
    }

    /// Content ref for a file, storing the blob when it is not inline.
    pub(super) fn content_ref(&self, bytes: &Bytes) -> pb::ContentRef {
        if bytes.len() <= INLINE_THRESHOLD {
            return pb::ContentRef {
                r#ref: Some(pb::content_ref::Ref::Inline(bytes.clone())),
            };
        }
        let hash = ObjectHash::digest(ObjectKind::Blob, bytes);
        self.blobs.lock().unwrap().insert(hash, bytes.clone());
        pb::ContentRef {
            r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                hash: hash_to_proto(&hash),
                size: bytes.len() as u64,
            })),
        }
    }

    pub(super) fn entry(&self, file: &MockFile) -> pb::TreeEntry {
        pb::TreeEntry {
            kind: Some(pb::tree_entry::Kind::File(pb::tree_entry::File {
                content: Some(self.content_ref(&file.bytes)),
                kind: pb::FileKind::from(file.kind) as i32,
            })),
        }
    }

    pub(super) fn record_auth<T>(&self, req: &Request<T>) {
        let header = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        self.auth_headers.lock().unwrap().push(header);
    }
}
