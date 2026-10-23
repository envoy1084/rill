//! Sending one delivery: a signed webhook POST, a Slack or Teams message,
//! or an email.

use std::sync::Arc;
use std::time::Duration;

use lettre::message::header::ContentType;
use lettre::{AsyncFileTransport, AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use rill_types::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::message::Message;
use super::sign;
use crate::config::NotifyConfig;
use crate::error::{Error, Result};

/// How long one HTTP delivery may take.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Where a delivery goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// A repository webhook, signed with the org secret named, if any.
    Webhook {
        url: String,
        secret: Option<String>,
    },
    Slack {
        url: String,
    },
    Teams {
        url: String,
    },
    Email {
        address: String,
    },
}

/// What a delivery carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Body {
    /// An event, for webhooks.
    Event(Value),
    /// A message, for people.
    Message(Message),
}

/// One queued delivery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delivery {
    /// Unique per delivery: the webhook's `webhook-id`.
    pub id: String,
    pub target: Target,
    pub body: Body,
}

/// Sends deliveries for one repository.
pub struct Deliverer {
    http: reqwest::Client,
    config: NotifyConfig,
    exec: Arc<rill_exec::remote::Scheduler>,
    org: String,
    repo: String,
}

fn failed(what: &str, e: impl std::fmt::Display) -> Error {
    Error::internal(format!("{what}: {e}"))
}

impl Deliverer {
    pub fn new(
        config: NotifyConfig,
        exec: Arc<rill_exec::remote::Scheduler>,
        org: &str,
        repo: &str,
    ) -> Self {
        Deliverer {
            http: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_default(),
            config,
            exec,
            org: org.to_string(),
            repo: repo.to_string(),
        }
    }

    /// Send `d`; an error leaves the job to be retried.
    pub async fn deliver(&self, d: &Delivery) -> Result<()> {
        match (&d.target, &d.body) {
            (Target::Webhook { url, secret }, body) => {
                let payload = match body {
                    Body::Event(v) => v.clone(),
                    Body::Message(m) => {
                        json!({ "id": d.id, "type": "message", "title": m.title, "text": m.text })
                    }
                };
                let body = serde_json::to_vec(&payload).map_err(|e| failed("webhook body", e))?;
                let ts = Timestamp::now().millis() / 1000;
                let mut request = self
                    .http
                    .post(url)
                    .header("content-type", "application/json")
                    .header(sign::ID_HEADER, &d.id)
                    .header(sign::TIMESTAMP_HEADER, ts.to_string());
                if let Some(name) = secret {
                    let value = self
                        .exec
                        .server_secret(&self.org, name, &self.repo)
                        .await
                        .map_err(|e| failed("webhook secret", e))?;
                    request = request.header(
                        sign::SIGNATURE_HEADER,
                        sign::signature(&value, &d.id, ts, &body),
                    );
                }
                self.post(request.body(body)).await
            }
            (Target::Slack { url } | Target::Teams { url }, Body::Message(m)) => {
                let request = self.http.post(url).json(&json!({ "text": m.line() }));
                self.post(request).await
            }
            (Target::Email { address }, Body::Message(m)) => self.email(address, m).await,
            (target, _) => Err(Error::internal(format!("nothing to send to {target:?}"))),
        }
    }

    async fn post(&self, request: reqwest::RequestBuilder) -> Result<()> {
        let response = request.send().await.map_err(|e| failed("delivery", e))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(failed("delivery", response.status()))
        }
    }

    async fn email(&self, address: &str, m: &Message) -> Result<()> {
        let Some(url) = &self.config.smtp_url else {
            return Err(Error::precondition("no SMTP server is configured"));
        };
        let email = lettre::Message::builder()
            .from(self.config.from.parse().map_err(|e| failed("sender", e))?)
            .to(address.parse().map_err(|e| failed("recipient", e))?)
            .subject(&m.title)
            .header(ContentType::TEXT_PLAIN)
            .body(m.text.clone())
            .map_err(|e| failed("email", e))?;
        match url.strip_prefix("file://") {
            Some(dir) => {
                AsyncFileTransport::<Tokio1Executor>::new(dir)
                    .send(email)
                    .await
                    .map_err(|e| failed("email", e))?;
            }
            None => {
                AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
                    .map_err(|e| failed("smtp", e))?
                    .build()
                    .send(email)
                    .await
                    .map_err(|e| failed("email", e))?;
            }
        }
        Ok(())
    }
}
