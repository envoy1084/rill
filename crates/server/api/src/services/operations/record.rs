//! Storing and loading operation records.

use buffa::Message;
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{OperationId, Timestamp};

use crate::services::common::Opened;
use crate::services::names;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

/// A new, unfinished operation of `kind` the caller started.
pub fn started(opened: &Opened, id: &OperationId, kind: &str) -> pb::Operation {
    let now = timestamp_to_proto(Timestamp::now());
    pb::Operation {
        name: names::operation(&opened.repo, id),
        kind: kind.into(),
        done: false,
        creator: pb::Principal::from(opened.principal().clone()).into(),
        created_at: now,
        updated_at: now,
        result: None,
    }
}

/// An error as an operation's failure.
pub fn status_of(err: Error) -> rill_proto::google::rpc::Status {
    rill_runtime::error::status_proto(err)
}

/// Store `operation` as `id`.
pub async fn save(ctx: &RepoContext, id: &OperationId, operation: &pb::Operation) -> Result<()> {
    ctx.repo
        .put_long_operation(id, operation.encode_to_vec().into())
        .await?;
    Ok(())
}

/// Operation `id`.
pub(super) async fn load(ctx: &RepoContext, id: &OperationId) -> Result<pb::Operation> {
    let bytes = ctx
        .repo
        .long_operation(id)
        .await?
        .ok_or_else(|| Error::not_found(format!("operation {id}")))?;
    decode(&bytes)
}

/// Every operation of the repository, newest first.
pub(super) async fn all(ctx: &RepoContext) -> Result<Vec<pb::Operation>> {
    let mut out = ctx
        .repo
        .long_operations()
        .await?
        .iter()
        .map(|b| decode(b))
        .collect::<Result<Vec<_>>>()?;
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.name.cmp(&b.name)));
    Ok(out)
}

fn decode(bytes: &[u8]) -> Result<pb::Operation> {
    pb::Operation::decode_from_slice(bytes)
        .map_err(|e| Error::internal(format!("stored operation: {e}")))
}
