//! `rill.v1.NotificationService`: repository webhooks and personal
//! notification preferences.

mod prefs;
mod service;
mod webhooks;

pub use service::NotificationSvc;
