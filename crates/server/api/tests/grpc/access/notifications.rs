//! Notifications: a repository webhook receives the events it asked for,
//! signed with an org secret; a person's channels get the messages that
//! concern them (Slack and email here); a test message reaches each one.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::Router;
use rill_runtime::notify::sign;

use super::super::*;
use super::fixture::*;
use super::writes::{change_of, edit};

/// A delivery the receiver got: its path, headers and body.
type Delivery = (String, HeaderMap, Vec<u8>);

#[derive(Clone, Default)]
struct Received(Arc<Mutex<Vec<Delivery>>>);

async fn receiver() -> (String, Received) {
    let got = Received::default();
    let app = Router::new()
        .route(
            "/{kind}",
            post(
                |State(got): State<Received>,
                 Path(kind): Path<String>,
                 headers: HeaderMap,
                 body: Bytes| async move {
                    got.0.lock().unwrap().push((kind, headers, body.to_vec()));
                },
            ),
        )
        .with_state(got.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, got)
}

async fn wait_for(got: &Received, kind: &str, n: usize) -> Vec<(HeaderMap, Vec<u8>)> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let seen: Vec<_> = got
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _, _)| k == kind)
            .map(|(_, h, b)| (h.clone(), b.clone()))
            .collect();
        if seen.len() >= n {
            return seen;
        }
        assert!(std::time::Instant::now() < deadline, "no {kind} delivery");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn webhooks_are_signed_and_people_hear_about_their_changes() {
    let mail = tempfile::tempdir().unwrap();
    let mut config = ServerConfig::default();
    config.notify.allow_insecure_urls = true;
    config.notify.smtp_url = Some(format!("file://{}", mail.path().display()));
    let mut acl = Acl::start_with(config).await;
    let (url, got) = receiver().await;
    let secret = b"whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
    let olga = add_user(
        &acl.server,
        "olga",
        vec![rill_types::identity::OrgRole::Admin],
        &[],
    )
    .await;
    acl.as_principal(&olga)
        .await
        .secrets
        .update_secret(pb::UpdateSecretRequest {
            secret: (pb::Secret {
                name: "orgs/default/secrets/HOOK_KEY".into(),
                ..Default::default()
            })
            .into(),
            value: Bytes::from_static(secret),
            allow_missing: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let hook = acl
        .owner
        .notifications
        .create_webhook(pb::CreateWebhookRequest {
            parent: common::names::repo("acl"),
            webhook: (pb::Webhook {
                url: format!("{url}/hook"),
                secret: "HOOK_KEY".into(),
                events: vec!["review_submitted".into()],
                ..Default::default()
            })
            .into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .webhook
        .unwrap();
    acl.grant("pub", "writer", &bob()).await;
    let mut bob_c = acl.as_principal(&bob()).await;
    assert_eq!(
        bob_c
            .notifications
            .create_webhook(pb::CreateWebhookRequest {
                parent: common::names::repo("acl"),
                webhook: (pb::Webhook {
                    url: format!("{url}/x"),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .code,
        Code::PermissionDenied
    );
    use pb::notification_channel::Channel;
    bob_c
        .notifications
        .update_notification_prefs(pb::UpdateNotificationPrefsRequest {
            parent: common::names::repo("acl"),
            prefs: (pb::NotificationPrefs {
                channels: vec![
                    pb::NotificationChannel {
                        channel: Some(Channel::Slack(format!("{url}/slack"))),
                    },
                    pb::NotificationChannel {
                        channel: Some(Channel::Email("bob@example.com".into())),
                    },
                ],
                topics: vec!["own_changes".into()],
                digest: false,
            })
            .into(),
            ..Default::default()
        })
        .await
        .unwrap();

    let v = edit(&mut bob_c, None, vec![set_edit("pub/b.rs", b"fn b() {}\n")])
        .await
        .unwrap();
    let change = change_of(&mut bob_c, &v).await;
    bob_c
        .review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap();
    let (headers, body) = wait_for(&got, "hook", 1).await.remove(0);
    let id = headers[sign::ID_HEADER].to_str().unwrap();
    let ts: i64 = headers[sign::TIMESTAMP_HEADER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        headers[sign::SIGNATURE_HEADER].to_str().unwrap(),
        sign::signature(secret, id, ts, &body)
    );
    let event: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(event["type"], "review_submitted");

    acl.owner
        .review
        .create_thread(pb::CreateThreadRequest {
            parent: common::names::review("acl", &change),
            body: "why b?".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let (_, slack) = wait_for(&got, "slack", 1).await.remove(0);
    let text: serde_json::Value = serde_json::from_slice(&slack).unwrap();
    assert!(
        text["text"]
            .as_str()
            .unwrap()
            .contains("opened a thread on your change"),
        "{text}"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::fs::read_dir(mail.path()).unwrap().count() == 0 {
        assert!(std::time::Instant::now() < deadline, "no email written");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let queued = bob_c
        .notifications
        .send_test_notification(pb::SendTestNotificationRequest {
            parent: common::names::repo("acl"),
        })
        .await
        .unwrap()
        .into_owned()
        .queued;
    assert_eq!(queued, 2);
    acl.owner
        .notifications
        .delete_webhook(pb::DeleteWebhookRequest {
            name: hook.name,
            ..Default::default()
        })
        .await
        .unwrap();
    acl.server.stop().await;
}
