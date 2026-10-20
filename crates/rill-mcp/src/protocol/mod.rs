//! JSON-RPC 2.0 messages and the newline-delimited stdio framing MCP uses.

mod framing;
mod rpc;

pub use framing::{read_message, write_message};
pub use rpc::{
    Id, Request, Response, RpcError, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST,
    METHOD_NOT_FOUND, PARSE_ERROR,
};

/// The MCP protocol revision this server speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
