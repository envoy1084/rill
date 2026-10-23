//! `rill notify`: repository webhooks, your notification channels, and a
//! test message.

use clap::{Args, Subcommand};
use rill_sdk::pb;
use serde_json::json;

use crate::context::Ctx;
use crate::output::Table;
use crate::Result;

/// `rill notify <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum NotifyCmd {
    /// The repository's webhooks (repository owners).
    #[command(subcommand)]
    Webhook(WebhookCmd),
    /// Show or change where your notifications go.
    Prefs(PrefsArgs),
    /// Send a test message to each of your channels.
    Test,
}

/// `rill notify webhook <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum WebhookCmd {
    /// Add a webhook.
    Add(WebhookAddArgs),
    /// List the webhooks.
    List,
    /// Remove a webhook.
    Rm(WebhookRmArgs),
}

/// `rill notify webhook add`.
#[derive(Args, Debug)]
pub struct WebhookAddArgs {
    /// Where events are POSTed (`https://`).
    pub url: String,
    /// The org secret that signs deliveries (Standard Webhooks).
    #[arg(long)]
    pub secret: Option<String>,
    /// An event kind to send (`landed`, `review_submitted`, …); repeatable;
    /// every event when absent.
    #[arg(long = "event")]
    pub events: Vec<String>,
}

/// `rill notify webhook rm`.
#[derive(Args, Debug)]
pub struct WebhookRmArgs {
    /// The webhook id.
    pub id: String,
}

/// `rill notify prefs`.
#[derive(Args, Debug)]
pub struct PrefsArgs {
    /// A Slack incoming-webhook URL.
    #[arg(long)]
    pub slack: Option<String>,
    /// A Microsoft Teams incoming-webhook URL.
    #[arg(long)]
    pub teams: Option<String>,
    /// An email address.
    #[arg(long)]
    pub email: Option<String>,
    /// A topic to hear about (`addressed`, `own_changes`, `audits`);
    /// repeatable; all when absent.
    #[arg(long = "topic")]
    pub topics: Vec<String>,
    /// A daily digest of the intents you own.
    #[arg(long)]
    pub digest: bool,
    /// Remove every channel.
    #[arg(long, conflicts_with_all = ["slack", "teams", "email"])]
    pub clear: bool,
}

fn tail(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

fn channel_text(c: &pb::NotificationChannel) -> String {
    use pb::notification_channel::Channel;
    match &c.channel {
        Some(Channel::Slack(u)) => format!("slack {u}"),
        Some(Channel::Teams(u)) => format!("teams {u}"),
        Some(Channel::Email(a)) => format!("email {a}"),
        None => String::new(),
    }
}

fn prefs_json(p: &pb::NotificationPrefs) -> serde_json::Value {
    json!({
        "channels": p.channels.iter().map(channel_text).collect::<Vec<_>>(),
        "topics": p.topics,
        "digest": p.digest,
    })
}

/// Run `rill notify`.
pub async fn run(ctx: &mut Ctx<'_>, cmd: NotifyCmd) -> Result<()> {
    let notify = ctx.repo()?.notifications();
    match cmd {
        NotifyCmd::Webhook(WebhookCmd::Add(a)) => {
            let w = notify
                .create_webhook(&a.url, a.secret.as_deref(), &a.events)
                .await?;
            ctx.emit(json!({ "id": tail(&w.name), "url": w.url }), |out| {
                out.line(format!("webhook {} → {}", tail(&w.name), w.url))
            })
        }
        NotifyCmd::Webhook(WebhookCmd::List) => {
            let hooks = notify.webhooks().await?;
            let value = json!(hooks
                .iter()
                .map(|w| json!({ "id": tail(&w.name), "url": w.url, "secret": w.secret, "events": w.events }))
                .collect::<Vec<_>>());
            ctx.emit(value, |out| {
                if hooks.is_empty() {
                    return out.line(out.dim("no webhooks"));
                }
                let mut table = Table::new(&["ID", "URL", "SIGNED WITH", "EVENTS"]);
                for w in &hooks {
                    table.row(vec![
                        tail(&w.name).to_string(),
                        w.url.clone(),
                        w.secret.clone(),
                        if w.events.is_empty() {
                            "all".into()
                        } else {
                            w.events.join(",")
                        },
                    ]);
                }
                out.table(table)
            })
        }
        NotifyCmd::Webhook(WebhookCmd::Rm(a)) => {
            notify.delete_webhook(&a.id).await?;
            ctx.emit(json!({ "deleted": a.id }), |out| out.line("removed"))
        }
        NotifyCmd::Prefs(a) => {
            use pb::notification_channel::Channel;
            let changing = a.clear
                || a.slack.is_some()
                || a.teams.is_some()
                || a.email.is_some()
                || !a.topics.is_empty()
                || a.digest;
            let mut p = notify.prefs().await?;
            if changing {
                if a.clear {
                    p.channels.clear();
                }
                let added = [
                    a.slack.map(Channel::Slack),
                    a.teams.map(Channel::Teams),
                    a.email.map(Channel::Email),
                ];
                for c in added.into_iter().flatten() {
                    p.channels
                        .push(pb::NotificationChannel { channel: Some(c) });
                }
                if !a.topics.is_empty() {
                    p.topics = a.topics;
                }
                p.digest = a.digest || (p.digest && !a.clear);
                p = notify.set_prefs(p).await?;
            }
            ctx.emit(prefs_json(&p), |out| {
                if p.channels.is_empty() {
                    out.line(out.dim("no channels: you get no notifications"))?;
                }
                for c in &p.channels {
                    out.line(format!("  {}", channel_text(c)))?;
                }
                out.line(format!(
                    "topics: {}; digest: {}",
                    if p.topics.is_empty() {
                        "all".to_string()
                    } else {
                        p.topics.join(", ")
                    },
                    if p.digest { "daily" } else { "off" }
                ))
            })
        }
        NotifyCmd::Test => {
            let n = notify.send_test().await?;
            ctx.emit(json!({ "queued": n }), |out| {
                out.line(format!("sent a test message to {n} channel(s)"))
            })
        }
    }
}
