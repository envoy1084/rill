//! Edit requests and root specs the tests write with.

use bytes::Bytes;
use rill_proto::rill::v1 as pb;

fn inline(bytes: &[u8]) -> pb::ContentRef {
    pb::ContentRef {
        r#ref: Some(pb::content_ref::Ref::Inline(Bytes::copy_from_slice(bytes))),
    }
}

pub(crate) fn set_edit(path: &str, bytes: &[u8]) -> pb::Edit {
    pb::Edit {
        path: path.into(),
        op: Some(pb::edit::Op::SetContent(pb::SetContent {
            content: Some(inline(bytes)),
            kind: pb::FileKind::Regular as i32,
            copy_from: None,
        })),
    }
}

pub(crate) fn trunk() -> pb::RootSpec {
    pb::RootSpec {
        root: Some(pb::root_spec::Root::Ref("trunk".into())),
    }
}
