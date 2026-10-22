//! Edit requests and revisions the tests write with.

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
            content: inline(bytes).into(),
            kind: pb::FileKind::Regular.into(),
            copy_from: None.into(),
        })),
    }
}

/// The revision naming trunk's head.
pub(crate) fn trunk() -> String {
    "trunk".into()
}

/// An edit base at trunk's head.
pub(crate) fn trunk_base() -> pb::EditBase {
    pb::EditBase {
        base: Some(pb::edit_base::Base::Revision(trunk())),
    }
}
