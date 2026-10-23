//! Who wants what: a repository's webhooks (set by its owners) and each
//! person's notification preferences (set by themselves).

use bytes::Bytes;
use rill_store::RepoStore;
use rill_types::{Principal, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Webhooks a repository keeps.
pub const MAX_WEBHOOKS: usize = 20;
/// Channels one person keeps.
pub const MAX_CHANNELS: usize = 5;

const WEBHOOKS_KEY: &str = "webhooks";

fn prefs_key(principal: &Principal) -> String {
    format!("notify-prefs:{principal}")
}

/// A repository webhook: every event of the chosen kinds, signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    pub id: String,
    pub url: String,
    /// The org secret whose value signs deliveries; unsigned without one.
    pub secret: Option<String>,
    /// Event kinds (`landed`, `review_submitted`, …); empty is all.
    pub events: Vec<String>,
    pub created_by: Principal,
    pub created_at: Timestamp,
}

/// Where a person's notifications go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    /// A Slack incoming webhook.
    Slack {
        url: String,
    },
    /// A Microsoft Teams incoming webhook.
    Teams {
        url: String,
    },
    Email {
        address: String,
    },
}

/// What a person is told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Topic {
    /// Threads addressed to them (escalations included).
    Addressed,
    /// Their own changes: new threads, rejections, ready to land, landed.
    OwnChanges,
    /// Audits assigned to them.
    Audits,
}

impl Topic {
    pub const ALL: [Topic; 3] = [Topic::Addressed, Topic::OwnChanges, Topic::Audits];

    pub fn as_str(self) -> &'static str {
        match self {
            Topic::Addressed => "addressed",
            Topic::OwnChanges => "own_changes",
            Topic::Audits => "audits",
        }
    }

    pub fn parse(s: &str) -> Option<Topic> {
        Topic::ALL.into_iter().find(|t| t.as_str() == s)
    }
}

/// One person's preferences in a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    pub channels: Vec<Channel>,
    /// Topics wanted; empty is all.
    pub topics: Vec<Topic>,
    /// A daily digest of the intents they own.
    pub digest: bool,
}

impl Prefs {
    pub fn wants(&self, topic: Topic) -> bool {
        self.topics.is_empty() || self.topics.contains(&topic)
    }
}

/// Refuse a URL the server should not call: not `https://` (unless
/// `insecure` allows `http://`), or without a host.
pub fn check_url(url: &str, insecure: bool) -> Result<()> {
    let parsed = reqwest::Url::parse(url).map_err(|e| Error::invalid(format!("{url}: {e}")))?;
    match parsed.scheme() {
        "https" => {}
        "http" if insecure => {}
        other => {
            return Err(Error::invalid(format!(
                "{url}: notifications go to https:// URLs, not {other}://"
            )))
        }
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err(Error::invalid(format!("{url} has no host")));
    }
    Ok(())
}

/// Check `prefs` before storing them.
pub fn check_prefs(prefs: &Prefs, insecure: bool) -> Result<()> {
    if prefs.channels.len() > MAX_CHANNELS {
        return Err(Error::invalid(format!("at most {MAX_CHANNELS} channels")));
    }
    for c in &prefs.channels {
        match c {
            Channel::Slack { url } | Channel::Teams { url } => check_url(url, insecure)?,
            Channel::Email { address } => {
                address
                    .parse::<lettre::Address>()
                    .map_err(|e| Error::invalid(format!("{address}: {e}")))?;
            }
        }
    }
    Ok(())
}

fn codec(e: serde_json::Error) -> Error {
    Error::internal(format!("notification settings: {e}"))
}

/// The repository's webhooks.
pub async fn webhooks(repo: &RepoStore) -> Result<Vec<Webhook>> {
    match repo.get_misc(WEBHOOKS_KEY).await? {
        Some(b) => serde_json::from_slice(&b).map_err(codec),
        None => Ok(Vec::new()),
    }
}

/// Change the repository's webhooks with `f`, retrying when another
/// write came first.
pub async fn update_webhooks<T>(
    repo: &RepoStore,
    mut f: impl FnMut(&mut Vec<Webhook>) -> Result<T>,
) -> Result<T> {
    for _ in 0..rill_review::lifecycle::MAX_WRITE_ATTEMPTS {
        let current = repo.get_misc(WEBHOOKS_KEY).await?;
        let mut hooks: Vec<Webhook> = match &current {
            Some(b) => serde_json::from_slice(b).map_err(codec)?,
            None => Vec::new(),
        };
        let out = f(&mut hooks)?;
        let value = serde_json::to_vec(&hooks).map_err(codec)?;
        match repo
            .set_misc_cas(WEBHOOKS_KEY, current, Bytes::from(value))
            .await
        {
            Ok(()) => return Ok(out),
            Err(e) if e.is_conflict() => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(rill_store::Error::Conflict("webhooks kept changing".into()).into())
}

/// `principal`'s preferences; none set means none.
pub async fn prefs(repo: &RepoStore, principal: &Principal) -> Result<Prefs> {
    match repo.get_misc(&prefs_key(principal)).await? {
        Some(b) => serde_json::from_slice(&b).map_err(codec),
        None => Ok(Prefs::default()),
    }
}

/// Store `principal`'s preferences.
pub async fn set_prefs(repo: &RepoStore, principal: &Principal, prefs: &Prefs) -> Result<()> {
    let value = serde_json::to_vec(prefs).map_err(codec)?;
    repo.set_misc(&prefs_key(principal), Bytes::from(value))
        .await?;
    let name = principal.to_string();
    for _ in 0..rill_review::lifecycle::MAX_WRITE_ATTEMPTS {
        let current = repo.get_misc(DIGEST_INDEX).await?;
        let mut index: Vec<String> = match &current {
            Some(b) => serde_json::from_slice(b).map_err(codec)?,
            None => Vec::new(),
        };
        if prefs.digest == index.contains(&name) {
            return Ok(());
        }
        if prefs.digest {
            index.push(name.clone());
        } else {
            index.retain(|n| *n != name);
        }
        let value = serde_json::to_vec(&index).map_err(codec)?;
        match repo
            .set_misc_cas(DIGEST_INDEX, current, Bytes::from(value))
            .await
        {
            Ok(()) => return Ok(()),
            Err(e) if e.is_conflict() => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(rill_store::Error::Conflict("the digest list kept changing".into()).into())
}

const DIGEST_INDEX: &str = "notify-digest";

/// Who asked for daily digests, by display form.
pub async fn digest_index(repo: &RepoStore) -> Result<Vec<String>> {
    match repo.get_misc(DIGEST_INDEX).await? {
        Some(b) => serde_json::from_slice(&b).map_err(codec),
        None => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_unless_allowed() {
        assert!(check_url("https://hooks.slack.com/services/x", false).is_ok());
        assert!(check_url("http://127.0.0.1:9000/hook", false).is_err());
        assert!(check_url("http://127.0.0.1:9000/hook", true).is_ok());
        assert!(check_url("file:///etc/passwd", true).is_err());
        let bad = Prefs {
            channels: vec![Channel::Email {
                address: "not an address".into(),
            }],
            ..Prefs::default()
        };
        assert!(check_prefs(&bad, false).is_err());
    }
}
