//! Shared fixtures: the test server, its clients, and common requests.

mod agents;
mod clients;
mod edits;
mod http;
mod landing;
pub(crate) mod names;
mod server;

pub(crate) use agents::register_bot;
pub(crate) use clients::{
    channel, clients, clients_at, connect, Authed, Bearer, Clients, Messages, ReapiBearer,
    ReapiSvc, Svc,
};
pub(crate) use edits::{set_edit, trunk, trunk_base};
pub(crate) use http::{http_get, http_post};
pub(crate) use landing::{land, land_with, request_land, try_land_with};
pub(crate) use server::{dev_config, TestServer};
