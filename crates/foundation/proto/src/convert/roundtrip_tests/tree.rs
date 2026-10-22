//! Trees through the wire.

use rill_types::*;

use super::fixtures::wire;
use crate::rill::v1 as pb;

#[test]
fn tree_roundtrip_through_wire() {
    let mut tree = Tree::empty();
    tree.insert_file("a", ContentRef::inline(b"1".to_vec()), FileKind::Regular);
    tree.insert_file("link", ContentRef::inline(b"a".to_vec()), FileKind::Symlink);
    tree.insert_dir(
        "d",
        ObjectHash::digest(ObjectKind::Tree, b"d"),
        rill_types::objects::tree::DirSummary {
            file_count: 3,
            dir_count: 1,
            total_size: 99,
            max_depth: 2,
        },
    );
    let back = Tree::try_from(wire(pb::Tree::from(&tree))).unwrap();
    assert_eq!(back, tree);
}
