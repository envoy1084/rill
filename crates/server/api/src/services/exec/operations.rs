//! Operations on the wire: long-running operations carrying REAPI's
//! metadata and response.

use bytes::Bytes;
use prost::Message;
use rill_exec::remote::{BlobDigest, OperationRecord, Stage};
use rill_reapi_proto::longrunning::{operation, Operation};
use rill_reapi_proto::reapi::{execution_stage, ExecuteOperationMetadata, ExecuteResponse};

const METADATA_TYPE: &str =
    "type.googleapis.com/build.bazel.remote.execution.v2.ExecuteOperationMetadata";
const RESPONSE_TYPE: &str = "type.googleapis.com/build.bazel.remote.execution.v2.ExecuteResponse";

fn any(type_url: &str, value: Bytes) -> prost_types::Any {
    prost_types::Any {
        type_url: type_url.to_string(),
        value: value.to_vec(),
    }
}

fn stage_of(stage: Stage) -> execution_stage::Value {
    match stage {
        Stage::Queued => execution_stage::Value::Queued,
        Stage::Executing => execution_stage::Value::Executing,
        Stage::Completed => execution_stage::Value::Completed,
    }
}

fn metadata(stage: execution_stage::Value, action: BlobDigest) -> prost_types::Any {
    let meta = ExecuteOperationMetadata {
        stage: stage as i32,
        action_digest: Some(action.to_proto()),
        ..Default::default()
    };
    any(METADATA_TYPE, Bytes::from(meta.encode_to_vec()))
}

/// The wire form of a stored operation.
pub fn operation_of(op: &OperationRecord) -> Operation {
    let result = op
        .response
        .clone()
        .map(|r| operation::Result::Response(any(RESPONSE_TYPE, r)));
    Operation {
        name: op.name(),
        metadata: Some(metadata(stage_of(op.stage), op.action_digest())),
        done: op.stage == Stage::Completed,
        result,
    }
}

/// A finished operation for an action served from the cache.
pub fn cached(name: String, action: BlobDigest, response: ExecuteResponse) -> Operation {
    Operation {
        name,
        metadata: Some(metadata(execution_stage::Value::Completed, action)),
        done: true,
        result: Some(operation::Result::Response(any(
            RESPONSE_TYPE,
            Bytes::from(response.encode_to_vec()),
        ))),
    }
}
