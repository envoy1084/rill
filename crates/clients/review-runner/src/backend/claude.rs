//! Claude through the Messages API. Each job is one request that must
//! answer through a forced tool call.

use async_trait::async_trait;
use rill_sdk::{ReviewJob, ReviewReport};
use serde_json::{json, Value};

use super::claude_tools::{
    resolve_tool, review_tool, tool_input, ResolveAnswer, ReviewAnswer, RESOLVE_TOOL, REVIEW_TOOL,
};
use super::{vote_for, Backend, ResolveOutcome, ResolveTask};
use crate::error::{Error, Result};
use crate::packet::{render, system_prompt};

/// The API version the requests are written for.
const API_VERSION: &str = "2023-06-01";
/// The model a resolver runs when it has no reviewer profile.
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";

/// The Claude backend.
#[derive(Debug, Clone)]
pub struct Claude {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    max_tokens: u32,
    /// The model resolvers run.
    pub resolve_model: String,
}

impl Claude {
    /// A backend calling `base_url` with `api_key`.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, max_tokens: u32) -> Self {
        Claude {
            http: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            max_tokens,
            resolve_model: DEFAULT_MODEL.into(),
        }
    }

    async fn call(&self, model: &str, system: String, user: String, tool: Value) -> Result<Value> {
        let name = tool["name"].clone();
        let body = json!({
            "model": model,
            "max_tokens": self.max_tokens,
            "system": system,
            "tools": [tool],
            "tool_choice": { "type": "tool", "name": name },
            "messages": [{ "role": "user", "content": user }],
        });
        let response = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let value: Value = response.json().await?;
        if !status.is_success() {
            let message = value["error"]["message"].as_str().unwrap_or("no message");
            return Err(Error::Backend(format!("{status}: {message}")));
        }
        Ok(value)
    }
}

#[async_trait]
impl Backend for Claude {
    async fn review(&self, job: &ReviewJob) -> Result<ReviewReport> {
        let model = &job.profile.model;
        let response = self
            .call(
                model,
                system_prompt(&job.prompt),
                render(job),
                review_tool(),
            )
            .await?;
        let answer: ReviewAnswer = tool_input(&response, REVIEW_TOOL)?;
        let findings = answer.findings();
        // The model proposes; a blocking finding always votes against.
        let vote = match vote_for(&findings, job.profile.max_vote) {
            v if v < 0 => v,
            v if answer.vote < 0 => -v,
            v => v,
        };
        Ok(ReviewReport {
            findings,
            vote,
            summary: answer.summary,
            model: response["model"].as_str().unwrap_or(model).to_string(),
        })
    }

    async fn resolve(&self, task: &ResolveTask) -> Result<ResolveOutcome> {
        let mut user = String::new();
        for f in &task.files {
            user.push_str(&format!("<conflict path=\"{}\">\n", f.path));
            if let Some(base) = &f.base {
                user.push_str(&format!("<base>\n{base}</base>\n"));
            }
            for (i, side) in f.sides.iter().enumerate() {
                match side {
                    Some(text) => user.push_str(&format!("<side n=\"{i}\">\n{text}</side>\n")),
                    None => user.push_str(&format!("<side n=\"{i}\" deleted=\"true\"/>\n")),
                }
            }
            user.push_str("</conflict>\n");
        }
        let prompt = if task.prompt.trim().is_empty() {
            "You resolve merge conflicts. Combine the sides so every side's intent \
             survives; decline when that needs a judgment a human should make."
        } else {
            &task.prompt
        };
        let response = self
            .call(
                &self.resolve_model,
                system_prompt(prompt),
                user,
                resolve_tool(),
            )
            .await?;
        let answer: ResolveAnswer = tool_input(&response, RESOLVE_TOOL)?;
        if answer.decline {
            return Ok(ResolveOutcome::Declined(answer.rationale));
        }
        Ok(ResolveOutcome::Resolved {
            files: answer
                .files
                .into_iter()
                .map(|f| (f.path, f.content))
                .collect(),
            rationale: answer.rationale,
            model: response["model"]
                .as_str()
                .unwrap_or(&self.resolve_model)
                .to_string(),
        })
    }
}
