//! The kinds of caller the matrix probes with, and what each may reach.

use tonic::Code;

use super::classes::Class::{self, *};

/// Who is calling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Anonymous,
    /// Authenticated, with no relation to the repository.
    Stranger,
    /// Reader of `pub/` only.
    PubReader,
    /// Writer of `pub/` only.
    PubWriter,
    /// The owner, with a token narrowed to reading `pub/`.
    Narrowed,
    Owner,
}

/// Whether the permission layer lets `role` through to an RPC of `class`.
pub(super) fn passes(class: Class, role: Role) -> Option<bool> {
    use Role::*;
    let allowed = match (class, role) {
        (Open, _) | (Grant, _) => return None,
        (_, Anonymous) => false,
        (Anyone, _) => true,
        (Admin, _) => false,
        (_, Owner) => true,
        (_, Stranger) => false,
        (ReadAny, PubReader | PubWriter | Narrowed) => true,
        (WriteAny, PubWriter) => true,
        _ => false,
    };
    Some(allowed)
}

pub(super) fn refused(code: Code) -> bool {
    matches!(code, Code::PermissionDenied | Code::Unauthenticated)
}
