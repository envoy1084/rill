//! Shared fixtures: the test server, its clients, and common requests.

mod clients;
mod edits;
mod http;
mod landing;
mod server;

pub(crate) use clients::{clients, clients_at, connect, Bearer, Clients, Svc};
pub(crate) use edits::{set_edit, trunk};
pub(crate) use http::http_get;
pub(crate) use landing::{land, land_with, try_land_with};
pub(crate) use server::{dev_config, TestServer};
