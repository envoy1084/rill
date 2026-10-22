//! ByteStream: streamed reads and writes of CAS blobs of any size.

use std::pin::Pin;

use bytes::Bytes;
use futures::{Stream, StreamExt};
use rill_reapi_proto::bytestream::byte_stream_server::ByteStream;
use rill_reapi_proto::bytestream::{
    QueryWriteStatusRequest, QueryWriteStatusResponse, ReadRequest, ReadResponse, WriteRequest,
    WriteResponse,
};
use rill_runtime::error::Error;
use tonic::{Request, Response, Status, Streaming};

use super::access::admit;
use super::resource::{parse_read, parse_write};
use super::service::ExecSvc;

/// Bytes per read response.
const READ_CHUNK: u64 = 1 << 20;

#[tonic::async_trait]
impl ByteStream for ExecSvc {
    type ReadStream = Pin<Box<dyn Stream<Item = Result<ReadResponse, Status>> + Send>>;

    async fn read(
        &self,
        request: Request<ReadRequest>,
    ) -> Result<Response<Self::ReadStream>, Status> {
        let resource = parse_read(&request.get_ref().resource_name)?;
        let caller = admit(&self.state, &request, &resource.instance).await?;
        let digest = resource.digest;
        let offset = request.get_ref().read_offset.max(0) as u64;
        if offset > digest.size {
            return Err(Status::out_of_range("read_offset past the end of the blob"));
        }
        let limit = match request.get_ref().read_limit {
            0 => digest.size - offset,
            l if l < 0 => return Err(Status::invalid_argument("negative read_limit")),
            l => (l as u64).min(digest.size - offset),
        };
        if caller
            .space
            .missing(&[digest])
            .await
            .map_err(Error::from)?
            .contains(&digest)
        {
            // Not stored: for a worker, perhaps a command carrying secrets,
            // which is small and served whole.
            let held = match caller.worker {
                true => super::cas::read_as(&self.state, &caller, &digest)
                    .await
                    .map_err(Error::from)?,
                false => None,
            };
            let Some(bytes) = held else {
                return Err(Status::not_found(format!("blob {digest}")));
            };
            let data = bytes.slice(offset as usize..(offset + limit) as usize);
            let once = futures::stream::once(async move { Ok(ReadResponse { data }) });
            return Ok(Response::new(Box::pin(once)));
        }
        let space = caller.space;
        let end = offset + limit;
        let chunks = futures::stream::unfold(offset, move |at| {
            let space = space.clone();
            async move {
                if at >= end {
                    return None;
                }
                let len = READ_CHUNK.min(end - at);
                let item = match space.read_range(&digest, at, len).await {
                    Ok(Some(data)) => Ok(ReadResponse { data }),
                    Ok(None) => Err(Status::not_found(format!("blob {digest}"))),
                    Err(e) => Err(Status::from(Error::from(e))),
                };
                Some((item, at + len))
            }
        });
        Ok(Response::new(Box::pin(chunks)))
    }

    async fn write(
        &self,
        request: Request<Streaming<WriteRequest>>,
    ) -> Result<Response<WriteResponse>, Status> {
        let (meta, extensions, mut stream) = request.into_parts();
        let first = stream
            .message()
            .await?
            .ok_or_else(|| Status::invalid_argument("empty write"))?;
        let resource = parse_write(&first.resource_name)?;
        let probe = Request::from_parts(meta, extensions, ());
        let caller = admit(&self.state, &probe, &resource.instance).await?;
        let digest = resource.digest;
        let committed = WriteResponse {
            committed_size: digest.size as i64,
        };
        if !caller
            .space
            .missing(&[digest])
            .await
            .map_err(Error::from)?
            .contains(&digest)
        {
            return Ok(Response::new(committed));
        }
        if first.write_offset != 0 {
            return Err(Status::invalid_argument("writes start at offset 0"));
        }
        let mut expected = first.data.len() as i64;
        let mut finished = first.finish_write;
        let head = futures::stream::iter([Ok(first.data)]);
        let rest = stream.map(move |message| {
            let message = message.map_err(|e| rill_core::Error::Upload(e.to_string()))?;
            if finished {
                return Err(rill_core::Error::Upload("data after finish_write".into()));
            }
            if message.write_offset != expected {
                return Err(rill_core::Error::Upload(format!(
                    "write_offset {} where {expected} was expected",
                    message.write_offset
                )));
            }
            expected += message.data.len() as i64;
            finished = message.finish_write;
            Ok::<Bytes, rill_core::Error>(message.data)
        });
        let parts = head.chain(rest);
        caller
            .space
            .write_stream(&digest, Box::pin(parts))
            .await
            .map_err(Error::from)?;
        Ok(Response::new(committed))
    }

    async fn query_write_status(
        &self,
        request: Request<QueryWriteStatusRequest>,
    ) -> Result<Response<QueryWriteStatusResponse>, Status> {
        let resource = parse_write(&request.get_ref().resource_name)?;
        let caller = admit(&self.state, &request, &resource.instance).await?;
        let digest = resource.digest;
        let complete = !caller
            .space
            .missing(&[digest])
            .await
            .map_err(Error::from)?
            .contains(&digest);
        Ok(Response::new(QueryWriteStatusResponse {
            committed_size: if complete { digest.size as i64 } else { 0 },
            complete,
        }))
    }
}
