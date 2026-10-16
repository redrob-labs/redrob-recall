// SPDX-License-Identifier: Apache-2.0
//! Ask's other half: one OpenAI-compatible chat request to the Redrob console, with the account's
//! key as the bearer token, model `auto`. The console's own `error.message` is what a failure shows
//! -- it says what is wrong (balance, a revoked key, a model that is not available) better than a
//! sentence written here could.

use crate::account::Account;
use redrob_recall_core::{AnswerSource, AskContext, AskResponse};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleError {
    pub code: String,
    pub message: String,
    pub http_status: u16,
}

impl std::fmt::Display for ConsoleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConsoleError {}

impl ConsoleError {
    fn new(code: &str, message: impl Into<String>, http_status: u16) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            http_status,
        }
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    temperature: f32,
    messages: [ChatMessage<'a>; 2],
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    model: Option<String>,
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

/// `{"error": {"message": "...", "type": "...", "code": "..."}}` or `{"error": "..."}`: the shapes
/// the console (and every OpenAI-compatible server) answers errors with.
#[derive(Deserialize)]
struct ErrorEnvelope {
    error: Option<ErrorValue>,
    message: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ErrorValue {
    Object {
        message: Option<String>,
        code: Option<serde_json::Value>,
    },
    Text(String),
}

/// The upstream message out of an error body, or `None` when the body has none we recognise.
pub fn upstream_message(body: &[u8]) -> Option<(String, Option<String>)> {
    let envelope: ErrorEnvelope = serde_json::from_slice(body).ok()?;
    match envelope.error {
        Some(ErrorValue::Object { message, code }) => {
            let message = message.filter(|m| !m.trim().is_empty())?;
            let code = code.map(|c| match c {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            });
            Some((message, code))
        }
        Some(ErrorValue::Text(text)) if !text.trim().is_empty() => Some((text, None)),
        _ => envelope
            .message
            .filter(|m| !m.trim().is_empty())
            .map(|m| (m, None)),
    }
}

/// Send the evidence and the question; answer with the model's text and the sources it cited from.
pub async fn ask(
    http: &reqwest::Client,
    account: &Account,
    model: &str,
    evidence: AskContext,
) -> Result<AskResponse, ConsoleError> {
    let url = format!("{}/chat/completions", account.api_base_url());
    let body = ChatRequest {
        model,
        temperature: 0.2,
        messages: [
            ChatMessage {
                role: "system",
                content: &evidence.system_prompt,
            },
            ChatMessage {
                role: "user",
                content: &evidence.user_prompt,
            },
        ],
    };
    let started = std::time::Instant::now();
    let response = http
        .post(&url)
        .bearer_auth(&account.stored.api_key)
        .header("x-redrob-app", "recall")
        .timeout(Duration::from_secs(120))
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            let code = if error.is_timeout() { "timeout" } else { "network_error" };
            ConsoleError::new(code, format!("Redrob could not be reached: {error}"), 503)
        })?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES)
    {
        return Err(ConsoleError::new(
            "invalid_response",
            "Redrob returned an unexpectedly large response.",
            502,
        ));
    }
    let raw = response.bytes().await.map_err(|error| {
        ConsoleError::new(
            "invalid_response",
            format!("The Redrob response could not be read: {error}"),
            502,
        )
    })?;
    if raw.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(ConsoleError::new(
            "invalid_response",
            "Redrob returned an unexpectedly large response.",
            502,
        ));
    }
    if !status.is_success() {
        // The console's own words first; a bare status only when it sent none.
        let (message, code) = upstream_message(&raw).unwrap_or_else(|| {
            (
                format!("Redrob answered {status} with no message."),
                None,
            )
        });
        let code = code.unwrap_or_else(|| match status.as_u16() {
            401 | 403 => "authentication_required".into(),
            402 => "insufficient_balance".into(),
            429 => "rate_limited".into(),
            500..=599 => "service_unavailable".into(),
            _ => "request_rejected".into(),
        });
        return Err(ConsoleError::new(&code, message, status.as_u16()));
    }
    let completion: ChatResponse = serde_json::from_slice(&raw).map_err(|error| {
        tracing::warn!(%error, "Redrob returned an unexpected response shape");
        ConsoleError::new(
            "invalid_response",
            "Redrob returned an unexpected response. Try again later.",
            502,
        )
    })?;
    let answer = completion
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .filter(|answer| !answer.trim().is_empty())
        .ok_or_else(|| {
            ConsoleError::new(
                "empty_response",
                "Redrob returned no answer. Try a more specific question.",
                502,
            )
        })?;
    let sources: Vec<AnswerSource> = evidence.sources;
    Ok(AskResponse {
        answer,
        sources,
        model: completion.model.unwrap_or_else(|| model.to_string()),
        input_tokens: completion.usage.as_ref().and_then(|usage| usage.prompt_tokens),
        output_tokens: completion
            .usage
            .as_ref()
            .and_then(|usage| usage.completion_tokens),
        latency_ms: started.elapsed().as_millis() as u64,
    })
}
