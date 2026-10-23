//! Turning events into notifications: every event a repository webhook
//! asks for, and messages to the people an event concerns (the change's
//! owner, whoever a thread addresses).

use std::sync::Arc;

use async_trait::async_trait;
use rill_events::EventFilter;
use rill_types::{Event, EventKind, ReviewStatus};

use super::consumer::Consumer;
use crate::error::Result;
use crate::notify::deliver::{Body, Delivery, Target};
use crate::notify::jobs::delivery_id;
use crate::notify::message::event_body;
use crate::notify::{enqueue, notify_people, prefs, Message, Topic};
use crate::state::RepoContext;

/// Sends notifications.
pub struct Notifier;

fn first_line(s: &str) -> String {
    s.lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(140)
        .collect()
}

#[async_trait]
impl Consumer for Notifier {
    fn name(&self) -> &'static str {
        "notifier"
    }

    fn filter(&self) -> EventFilter {
        EventFilter::default()
    }

    async fn handle(&self, ctx: &Arc<RepoContext>, event: &Event) -> Result<()> {
        for hook in prefs::webhooks(&ctx.repo).await? {
            if !hook.events.is_empty() && !hook.events.iter().any(|k| k == event.kind.name()) {
                continue;
            }
            let id = delivery_id();
            let body = event_body(&ctx.config.name, &id, event);
            enqueue(
                ctx,
                &Delivery {
                    id,
                    target: Target::Webhook {
                        url: hook.url,
                        secret: hook.secret,
                    },
                    body: Body::Event(body),
                },
            )
            .await?;
        }
        personal(ctx, event).await
    }
}

async fn personal(ctx: &RepoContext, event: &Event) -> Result<()> {
    let Some(change) = event.kind.change() else {
        return Ok(());
    };
    let Some(state) = ctx.repo.get_review(&change).await? else {
        return Ok(());
    };
    let repo = &ctx.config.name;
    let owner = vec![state.owner.to_string()];
    let actor_is_owner = event.actor == state.owner;
    match &event.kind {
        EventKind::Commented {
            thread, new_thread, ..
        } => {
            let Some(t) = state.threads.iter().find(|t| t.id == *thread) else {
                return Ok(());
            };
            let body = t
                .comments
                .last()
                .map_or(String::new(), |c| first_line(&c.body));
            if let Some(to) = t.to.as_ref().filter(|to| **to != event.actor) {
                let m = Message::new(
                    format!("{} wrote to you on {change} in {repo}", event.actor),
                    body.clone(),
                );
                notify_people(ctx, &[to.to_string()], Some(Topic::Addressed), &m).await?;
            }
            if *new_thread && !actor_is_owner {
                let m = Message::new(
                    format!("{} opened a thread on your change {change}", event.actor),
                    body,
                );
                notify_people(ctx, &owner, Some(Topic::OwnChanges), &m).await?;
            }
        }
        EventKind::Voted { label, value, .. } if *value < 0 && !actor_is_owner => {
            let m = Message::new(
                format!(
                    "{} voted {label} {value} on your change {change}",
                    event.actor
                ),
                String::new(),
            );
            notify_people(ctx, &owner, Some(Topic::OwnChanges), &m).await?;
        }
        EventKind::StatusChanged {
            to: ReviewStatus::Landable,
            ..
        } => {
            let m = Message::new(
                format!("Your change {change} in {repo} can land"),
                String::new(),
            );
            notify_people(ctx, &owner, Some(Topic::OwnChanges), &m).await?;
        }
        EventKind::Landed { seq, .. } => {
            let m = Message::new(
                format!("Your change {change} landed in {repo} at trunk@{}", seq.0),
                String::new(),
            );
            notify_people(ctx, &owner, Some(Topic::OwnChanges), &m).await?;
        }
        _ => {}
    }
    Ok(())
}
