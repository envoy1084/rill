//! The CLI against an in-process server and client daemon: one test binary,
//! one module per topic.

mod common;

mod access;
mod cache;
mod cli_e2e;
mod commands;
mod doctor;
mod human;
mod import;
mod intents;
mod keycloak;
mod landing;
mod login;
mod owners;
mod parse;
mod reshape;
mod schemas;
mod tls;
