//! Calling any RPC by name with hand-encoded protobuf, for the permission
//! matrix: every method of every service, from the embedded descriptor.

use std::time::Duration;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use prost::Message;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::transport::Channel;
use tonic::{Code, Request, Status};

/// One RPC as the descriptor describes it.
#[derive(Debug, Clone)]
pub struct Method {
    /// `rill.v1.RepoService/ListDir`.
    pub name: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    /// The input message's `repo` field number, when it has one.
    pub repo_field: Option<i32>,
    /// The input message's `workspace` field number, when it has one.
    pub workspace_field: Option<i32>,
}

/// Every method of every service in `rill.v1`.
pub fn methods() -> Vec<Method> {
    let fds = prost_types::FileDescriptorSet::decode(rill_proto::FILE_DESCRIPTOR_SET).unwrap();
    let messages: std::collections::HashMap<String, &prost_types::DescriptorProto> = fds
        .file
        .iter()
        .flat_map(|f| {
            let package = f.package().to_string();
            f.message_type
                .iter()
                .map(move |m| (format!(".{package}.{}", m.name()), m))
        })
        .collect();
    let mut out = Vec::new();
    for file in &fds.file {
        for service in &file.service {
            for method in &service.method {
                let input = messages[method.input_type()];
                let field = |name: &str, ty: prost_types::field_descriptor_proto::Type| {
                    input
                        .field
                        .iter()
                        .find(|f| f.name() == name && f.r#type() == ty)
                        .map(|f| f.number())
                };
                use prost_types::field_descriptor_proto::Type;
                let repo_field = field("repo", Type::String);
                let workspace_field = field("workspace", Type::Bytes);
                out.push(Method {
                    name: format!("{}.{}/{}", file.package(), service.name(), method.name()),
                    client_streaming: method.client_streaming(),
                    server_streaming: method.server_streaming(),
                    repo_field,
                    workspace_field,
                });
            }
        }
    }
    out
}

/// A request with only `repo` and `workspace` set, where the message has
/// them.
pub fn request_body(method: &Method, repo: &str, workspace: &[u8]) -> Bytes {
    let mut out = BytesMut::new();
    if let Some(field) = method.repo_field {
        prost::encoding::string::encode(field as u32, &repo.to_string(), &mut out);
    }
    if let Some(field) = method.workspace_field {
        prost::encoding::bytes::encode(field as u32, &workspace.to_vec(), &mut out);
    }
    out.freeze()
}

#[derive(Clone, Default)]
struct RawCodec;

struct RawCoder;

impl Encoder for RawCoder {
    type Item = Bytes;
    type Error = Status;
    fn encode(&mut self, item: Bytes, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        dst.put(item);
        Ok(())
    }
}

impl Decoder for RawCoder {
    type Item = Bytes;
    type Error = Status;
    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Bytes>, Status> {
        Ok(Some(src.copy_to_bytes(src.remaining())))
    }
}

impl Codec for RawCodec {
    type Encode = Bytes;
    type Decode = Bytes;
    type Encoder = RawCoder;
    type Decoder = RawCoder;
    fn encoder(&mut self) -> RawCoder {
        RawCoder
    }
    fn decoder(&mut self) -> RawCoder {
        RawCoder
    }
}

/// What a call came to: its status code, and the bytes it returned (the
/// first message of a stream) or its error message.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub code: Code,
    pub body: Bytes,
}

/// Call `method` with `body` as `token` (anonymous when `None`).
pub async fn call(channel: Channel, method: &Method, body: Bytes, token: Option<&str>) -> Outcome {
    let mut grpc = tonic::client::Grpc::new(channel);
    grpc.ready().await.unwrap();
    let mut request = Request::new(body);
    if let Some(token) = token {
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
    }
    let path: http::uri::PathAndQuery = format!("/{}", method.name).parse().unwrap();
    let failed = |s: Status| Outcome {
        code: s.code(),
        body: Bytes::copy_from_slice(s.message().as_bytes()),
    };
    if method.client_streaming {
        let (metadata, extensions, body) = request.into_parts();
        let stream = Request::from_parts(metadata, extensions, tokio_stream::iter([body]));
        return match grpc.client_streaming(stream, path, RawCodec).await {
            Ok(r) => Outcome {
                code: Code::Ok,
                body: r.into_inner(),
            },
            Err(s) => failed(s),
        };
    }
    if method.server_streaming {
        return match grpc.server_streaming(request, path, RawCodec).await {
            Ok(r) => {
                let mut stream = r.into_inner();
                match tokio::time::timeout(Duration::from_millis(300), stream.message()).await {
                    Ok(Ok(Some(first))) => Outcome {
                        code: Code::Ok,
                        body: first,
                    },
                    Ok(Err(s)) => failed(s),
                    _ => Outcome {
                        code: Code::Ok,
                        body: Bytes::new(),
                    },
                }
            }
            Err(s) => failed(s),
        };
    }
    match grpc.unary(request, path, RawCodec).await {
        Ok(r) => Outcome {
            code: Code::Ok,
            body: r.into_inner(),
        },
        Err(s) => failed(s),
    }
}
