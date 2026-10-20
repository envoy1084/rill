//! Encoded bytes of one canonical value per stored type, pinned in
//! `tests/fixtures/`. An encoding change fails here, which means the store
//! format must change too (bump `rill_store::STORE_FORMAT`). Regenerate with
//! `RILL_UPDATE_FIXTURES=1 cargo test -p rill-types --test golden`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use bytes::Bytes;
use rill_types::objects::blob::ChunkRef;
use rill_types::objects::tree::DirSummary;
use rill_types::{
    encode_record, BlobManifest, ChangeId, ChangeVersion, ContentRef, Encodable, FileChange,
    FileKind, IntentId, Merge, ObjectHash, Principal, Provenance, RepoId, RepoPathBuf,
    SparseProfile, Timestamp, Tree, TreeNode, Workspace, WorkspaceId,
};

fn hash(n: u8) -> ObjectHash {
    ObjectHash::from_bytes([n; 32])
}

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

fn change_version() -> ChangeVersion {
    let mut file_changes = BTreeMap::new();
    file_changes.insert(
        path("src/main.rs"),
        FileChange::Set {
            content: ContentRef::inline(Bytes::from_static(b"fn main() {}\n")),
            kind: FileKind::Regular,
            copy_from: None,
        },
    );
    file_changes.insert(path("old.txt"), FileChange::Delete);
    file_changes.insert(
        path("both.txt"),
        FileChange::Conflict(Merge::three_way(
            Some((
                ContentRef::Blob {
                    hash: hash(1),
                    size: 9,
                },
                FileKind::Regular,
            )),
            Some((
                ContentRef::Blob {
                    hash: hash(2),
                    size: 9,
                },
                FileKind::Regular,
            )),
            None,
        )),
    );
    ChangeVersion {
        change_id: ChangeId::from_bytes([7; 16]),
        parents: vec![hash(3)],
        predecessor: Some(hash(4)),
        subtree_changes: BTreeMap::new(),
        file_changes,
        author: Principal::agent("bot"),
        provenance: Provenance {
            tool: Some("rill-cli".into()),
            model: Some("model-x".into()),
            ..Provenance::default()
        },
        intent: Some(IntentId::from_bytes([8; 16])),
        message: "golden".into(),
        created_at: Timestamp::from_millis(1_700_000_000_000),
        extra: BTreeMap::new(),
    }
}

fn tree() -> Tree {
    let mut tree = Tree::empty();
    tree.insert_file(
        "README.md",
        ContentRef::inline(Bytes::from_static(b"# hi\n")),
        FileKind::Regular,
    );
    tree.insert_dir(
        "src",
        hash(5),
        DirSummary {
            file_count: 2,
            dir_count: 0,
            total_size: 40,
            max_depth: 0,
        },
    );
    tree
}

fn manifest() -> BlobManifest {
    BlobManifest {
        total_size: 96 * 1024,
        chunks: vec![
            ChunkRef {
                hash: hash(6),
                size: 64 * 1024,
            },
            ChunkRef {
                hash: hash(7),
                size: 32 * 1024,
            },
        ],
    }
}

fn workspace() -> Workspace {
    Workspace {
        id: WorkspaceId::from_bytes([9; 16]),
        repo: RepoId::from_bytes([10; 16]),
        owner: Principal::human("alice"),
        name: "main".into(),
        base: hash(11),
        working_change: ChangeId::from_bytes([12; 16]),
        snapshot: None,
        sparse: SparseProfile::full(),
        head_op: None,
        created_at: Timestamp::from_millis(1),
        updated_at: Timestamp::from_millis(2),
    }
}

fn check(name: &str, bytes: &[u8]) {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.bin"));
    if std::env::var_os("RILL_UPDATE_FIXTURES").is_some() {
        std::fs::write(&file, bytes).unwrap();
        return;
    }
    let pinned = std::fs::read(&file)
        .unwrap_or_else(|e| panic!("{}: {e}; run with RILL_UPDATE_FIXTURES=1", file.display()));
    assert!(
        pinned == bytes,
        "{name} encodes differently from {}; if intended, bump STORE_FORMAT and regenerate",
        file.display()
    );
}

#[test]
fn stored_types_encode_as_pinned() {
    check("change_version", &change_version().encode());
    check("tree", &TreeNode::Leaf(tree()).encode());
    check("blob_manifest", &manifest().encode());
    check("workspace_record", &encode_record(&workspace()));
}

#[test]
fn pinned_bytes_decode_to_the_canonical_values() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let read = |name: &str| std::fs::read(dir.join(format!("{name}.bin"))).unwrap();
    assert_eq!(
        ChangeVersion::decode(&read("change_version")).unwrap(),
        change_version()
    );
    assert_eq!(
        TreeNode::decode(&read("tree")).unwrap(),
        TreeNode::Leaf(tree())
    );
    assert_eq!(
        BlobManifest::decode(&read("blob_manifest")).unwrap(),
        manifest()
    );
    assert_eq!(
        rill_types::decode_record::<Workspace>(&read("workspace_record")).unwrap(),
        workspace()
    );
}
