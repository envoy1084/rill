//! Notifications: signed webhooks for a repository's events, and personal
//! messages (Slack, Teams, email) for what concerns someone, delivered
//! through a job queue with retries.

pub mod deliver;
pub mod jobs;
pub mod message;
pub mod prefs;
pub mod sign;

pub use jobs::{enqueue, notify_people, NOTIFY_QUEUE};
pub use message::Message;
pub use prefs::{Channel, Prefs, Topic, Webhook};
