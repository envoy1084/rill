//! A person's own notification preferences.

use rill_proto::rill::v1 as pb;
use rill_runtime::error::{Error, Result};
use rill_runtime::notify::deliver::{Body, Delivery, Target};
use rill_runtime::notify::jobs::delivery_id;
use rill_runtime::notify::prefs::{self, check_prefs};
use rill_runtime::notify::{enqueue, Channel, Message, Prefs, Topic};

use super::NotificationSvc;
use crate::services::common::open_repo;
use crate::services::Call;

fn prefs_proto(p: Prefs) -> pb::NotificationPrefs {
    use pb::notification_channel::Channel as C;
    pb::NotificationPrefs {
        channels: p
            .channels
            .into_iter()
            .map(|c| pb::NotificationChannel {
                channel: Some(match c {
                    Channel::Slack { url } => C::Slack(url),
                    Channel::Teams { url } => C::Teams(url),
                    Channel::Email { address } => C::Email(address),
                }),
            })
            .collect(),
        topics: p.topics.iter().map(|t| t.as_str().to_string()).collect(),
        digest: p.digest,
    }
}

fn prefs_from(p: pb::NotificationPrefs) -> Result<Prefs> {
    use pb::notification_channel::Channel as C;
    Ok(Prefs {
        channels: p
            .channels
            .into_iter()
            .map(|c| match c.channel {
                Some(C::Slack(url)) => Ok(Channel::Slack { url }),
                Some(C::Teams(url)) => Ok(Channel::Teams { url }),
                Some(C::Email(address)) => Ok(Channel::Email { address }),
                None => Err(Error::invalid("a channel needs slack, teams or email")),
            })
            .collect::<Result<_>>()?,
        topics: p
            .topics
            .iter()
            .map(|t| Topic::parse(t).ok_or_else(|| Error::invalid(format!("unknown topic {t:?}"))))
            .collect::<Result<_>>()?,
        digest: p.digest,
    })
}

impl NotificationSvc {
    pub(super) async fn get_notification_prefs_impl(
        &self,
        request: Call<pb::GetNotificationPrefsRequest>,
    ) -> Result<pb::GetNotificationPrefsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read_any().await?;
        let p = prefs::prefs(&opened.repo.repo, opened.principal()).await?;
        Ok(pb::GetNotificationPrefsResponse {
            prefs: prefs_proto(p).into(),
        })
    }

    pub(super) async fn update_notification_prefs_impl(
        &self,
        request: Call<pb::UpdateNotificationPrefsRequest>,
    ) -> Result<pb::UpdateNotificationPrefsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read_any().await?;
        let p = prefs_from(req.prefs.clone().into_option().unwrap_or_default())?;
        check_prefs(&p, self.state.config.notify.allow_insecure_urls)?;
        prefs::set_prefs(&opened.repo.repo, opened.principal(), &p).await?;
        Ok(pb::UpdateNotificationPrefsResponse {
            prefs: prefs_proto(p).into(),
        })
    }

    pub(super) async fn send_test_notification_impl(
        &self,
        request: Call<pb::SendTestNotificationRequest>,
    ) -> Result<pb::SendTestNotificationResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read_any().await?;
        let p = prefs::prefs(&opened.repo.repo, opened.principal()).await?;
        let message = Message::new(
            format!("Test from {}", opened.repo.config.name),
            "Notifications reach you here.",
        );
        let mut queued = 0;
        for c in p.channels {
            let target = match c {
                Channel::Slack { url } => Target::Slack { url },
                Channel::Teams { url } => Target::Teams { url },
                Channel::Email { address } => Target::Email { address },
            };
            enqueue(
                &opened.repo,
                &Delivery {
                    id: delivery_id(),
                    target,
                    body: Body::Message(message.clone()),
                },
            )
            .await?;
            queued += 1;
        }
        Ok(pb::SendTestNotificationResponse { queued })
    }
}
