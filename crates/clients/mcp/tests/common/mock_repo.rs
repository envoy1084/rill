//! The mock repository's state: trees, blobs and a canned log.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use bytes::Bytes;
use connectrpc::ConnectError as Status;
use rill_proto::convert::hash_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{ObjectHash, ObjectKind, INLINE_THRESHOLD};

/// Root label -> path -> bytes.
pub type Trees = HashMap<String, BTreeMap<String, Bytes>>;

#[derive(Default)]
pub struct MockRepo {
    pub trees: Mutex<Trees>,
    pub blobs: Mutex<HashMap<ObjectHash, Bytes>>,
    pub log: Mutex<Vec<pb::ChangeInfo>>,
}

impl MockRepo {
    pub fn set_tree(&self, label: &str, files: Vec<(&str, &[u8])>) {
        let map = files
            .into_iter()
            .map(|(p, b)| (p.to_string(), Bytes::copy_from_slice(b)))
            .collect();
        self.trees.lock().unwrap().insert(label.to_string(), map);
    }

    pub(super) fn root_label(root: Option<&pb::RootSpec>) -> Result<String, Status> {
        match root.and_then(|r| r.root.as_ref()) {
            Some(pb::root_spec::Root::Ref(name)) => Ok(name.clone()),
            Some(pb::root_spec::Root::VersionHash(h)) => Ok(hex::encode(h)),
            Some(pb::root_spec::Root::TreeHash(h)) => Ok(format!("tree:{}", hex::encode(h))),
            Some(pb::root_spec::Root::TrunkSeq(n)) => Ok(format!("trunk@{n}")),
            None => Err(Status::invalid_argument("missing root")),
        }
    }

    pub(super) fn tree(
        &self,
        root: Option<&pb::RootSpec>,
    ) -> Result<BTreeMap<String, Bytes>, Status> {
        let label = Self::root_label(root)?;
        self.trees
            .lock()
            .unwrap()
            .get(&label)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("root {label}")))
    }

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

    pub(super) fn entry(&self, bytes: &Bytes) -> pb::TreeEntry {
        pb::TreeEntry {
            kind: Some(pb::tree_entry::Kind::File(pb::tree_entry::File {
                content: self.content_ref(bytes).into(),
                kind: pb::FileKind::Regular.into(),
            })),
        }
    }
}
