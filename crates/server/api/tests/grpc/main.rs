//! End-to-end tests: an in-process server on ephemeral ports driven
//! through the generated clients and a raw HTTP client.

mod access;
mod agents;
mod audit;
mod auth;
mod cluster;
mod commits;
mod common;
mod connect;
mod encryption;
mod end_to_end;
mod events;
mod exec;
mod history;
mod identity;
mod keycloak;
#[cfg(feature = "saml")]
mod keycloak_saml;
mod killswitch;
mod kubernetes;
mod limits;
mod orgs;
mod protection;
mod releases;
mod replicas;
mod retries;
mod scim;
mod search;
mod search_shards;
mod shutdown;
mod signin;
mod stacks;
mod structural;
mod telemetry;
mod verification;
mod workspaces;

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use connectrpc::{ConnectError, ErrorCode as Code};
use futures::StreamExt;
use rill_api::{AppState, Listeners, ServerConfig};
use rill_proto::rill::v1 as pb;
use rill_types::{ObjectHash, ObjectKind};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

use common::{
    channel, clients, clients_at, connect, dev_config, http_get, land, land_with, set_edit, trunk,
    try_land_with, Authed, Bearer, Clients, Messages, Svc, TestServer,
};
