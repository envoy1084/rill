//! The content-addressable store: which blobs exist, and small blobs in
//! batches. Large blobs travel over ByteStream.

use std::pin::Pin;

use futures::Stream;
use rill_exec::remote::BlobDigest;
use rill_reapi_proto::reapi::content_addressable_storage_server::ContentAddressableStorage;
use rill_reapi_proto::reapi::{
    batch_read_blobs_response, batch_update_blobs_response, BatchReadBlobsRequest,
    BatchReadBlobsResponse, BatchUpdateBlobsRequest, BatchUpdateBlobsResponse,
    FindMissingBlobsRequest, FindMissingBlobsResponse, GetTreeRequest, GetTreeResponse,
    SpliceBlobRequest, SpliceBlobResponse, SplitBlobRequest, SplitBlobResponse,
};
use rill_reapi_proto::rpc;
use rill_runtime::error::Error;
use tonic::{Code, Request, Response, Status};

use super::access::admit;
use super::capabilities::MAX_BATCH_BYTES;
use super::service::ExecSvc;

fn rpc_status(code: Code, message: impl Into<String>) -> rpc::Status {
    rpc::Status {
        code: code as i32,
        message: message.into(),
        details: vec![],
    }
}

fn ok() -> rpc::Status {
    rpc_status(Code::Ok, "")
}

#[tonic::async_trait]
impl ContentAddressableStorage for ExecSvc {
    async fn find_missing_blobs(
        &self,
        request: Request<FindMissingBlobsRequest>,
    ) -> Result<Response<FindMissingBlobsResponse>, Status> {
        let caller = admit(&self.state, &request, &request.get_ref().instance_name).await?;
        let digests = request
            .get_ref()
            .blob_digests
            .iter()
            .map(BlobDigest::parse)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::from)?;
        let missing = caller.space.missing(&digests).await.map_err(Error::from)?;
        Ok(Response::new(FindMissingBlobsResponse {
            missing_blob_digests: missing.into_iter().map(BlobDigest::to_proto).collect(),
        }))
    }

    async fn batch_update_blobs(
        &self,
        request: Request<BatchUpdateBlobsRequest>,
    ) -> Result<Response<BatchUpdateBlobsResponse>, Status> {
        let caller = admit(&self.state, &request, &request.get_ref().instance_name).await?;
        let total: usize = request
            .get_ref()
            .requests
            .iter()
            .map(|r| r.data.len())
            .sum();
        if total as i64 > MAX_BATCH_BYTES {
            return Err(Status::invalid_argument(
                "batch exceeds max_batch_total_size_bytes",
            ));
        }
        let mut responses = Vec::new();
        for item in request.into_inner().requests {
            let status = match item.digest.as_ref().map(BlobDigest::parse) {
                Some(Ok(digest)) => match caller.space.write(&digest, item.data).await {
                    Ok(()) => ok(),
                    Err(rill_exec::Error::DigestMismatch(d)) => {
                        rpc_status(Code::InvalidArgument, format!("content does not match {d}"))
                    }
                    Err(e) => rpc_status(Code::Internal, e.to_string()),
                },
                _ => rpc_status(Code::InvalidArgument, "invalid digest"),
            };
            responses.push(batch_update_blobs_response::Response {
                digest: item.digest,
                status: Some(status),
            });
        }
        Ok(Response::new(BatchUpdateBlobsResponse { responses }))
    }

    async fn batch_read_blobs(
        &self,
        request: Request<BatchReadBlobsRequest>,
    ) -> Result<Response<BatchReadBlobsResponse>, Status> {
        let caller = admit(&self.state, &request, &request.get_ref().instance_name).await?;
        let mut responses = Vec::new();
        let mut total = 0i64;
        for digest in request.into_inner().digests {
            let parsed = BlobDigest::parse(&digest);
            let (data, status) = match parsed {
                Ok(d) if total + d.size as i64 > MAX_BATCH_BYTES => (
                    Default::default(),
                    rpc_status(Code::ResourceExhausted, "batch too large; use ByteStream"),
                ),
                Ok(d) => match read_as(&self.state, &caller, &d).await {
                    Ok(Some(data)) => {
                        total += data.len() as i64;
                        (data, ok())
                    }
                    Ok(None) => (
                        Default::default(),
                        rpc_status(Code::NotFound, format!("{d}")),
                    ),
                    Err(e) => (
                        Default::default(),
                        rpc_status(Code::Internal, e.to_string()),
                    ),
                },
                Err(_) => (
                    Default::default(),
                    rpc_status(Code::InvalidArgument, "invalid digest"),
                ),
            };
            responses.push(batch_read_blobs_response::Response {
                digest: Some(digest),
                data,
                compressor: 0,
                status: Some(status),
            });
        }
        Ok(Response::new(BatchReadBlobsResponse { responses }))
    }

    type GetTreeStream = Pin<Box<dyn Stream<Item = Result<GetTreeResponse, Status>> + Send>>;

    async fn get_tree(
        &self,
        request: Request<GetTreeRequest>,
    ) -> Result<Response<Self::GetTreeStream>, Status> {
        let caller = admit(&self.state, &request, &request.get_ref().instance_name).await?;
        let root = request
            .get_ref()
            .root_digest
            .clone()
            .ok_or_else(|| Status::invalid_argument("root_digest is required"))?;
        let directories = caller.space.tree(&root).await.map_err(Error::from)?;
        let page = GetTreeResponse {
            directories,
            next_page_token: String::new(),
        };
        Ok(Response::new(Box::pin(futures::stream::iter([Ok(page)]))))
    }

    async fn split_blob(
        &self,
        _request: Request<SplitBlobRequest>,
    ) -> Result<Response<SplitBlobResponse>, Status> {
        Err(Status::unimplemented("blob splitting is not supported"))
    }

    async fn splice_blob(
        &self,
        _request: Request<SpliceBlobRequest>,
    ) -> Result<Response<SpliceBlobResponse>, Status> {
        Err(Status::unimplemented("blob splicing is not supported"))
    }
}

/// `digest`'s content as `caller` may read it: workers also read the
/// commands that carry their actions' secrets.
pub(super) async fn read_as(
    state: &rill_runtime::state::AppState,
    caller: &super::access::ExecCaller,
    digest: &BlobDigest,
) -> rill_exec::Result<Option<bytes::Bytes>> {
    match caller.worker {
        true => state.exec.worker_read(&caller.space, digest).await,
        false => caller.space.read(digest).await,
    }
}
