use crate::{
    models::{AnswerSource, AskRequest, AskResponse, SearchFilters, SearchRequest},
    search,
    state::AppState,
};
use futures_util::StreamExt;
use reqwest::{header::RETRY_AFTER, StatusCode};
use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration};

const MAX_QUESTION_CHARACTERS: usize = 4_000;
const MAX_CONTEXT_CHARACTERS: usize = 60_000;
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedrobError {
    pub code: String,
    pub message: String,
    pub http_status: u16,
    pub retry_after_seconds: Option<u64>,
}

impl fmt::Display for RedrobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RedrobError {}

impl RedrobError {
    fn new(code: &str, message: &str, http_status: u16) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            http_status,
            retry_after_seconds: None,
        }
    }

    fn local(message: impl fmt::Display) -> Self {
        tracing::warn!(error = %message, "local Ask preparation failed");
        Self::new(
            "local_error",
            "The local evidence could not be prepared. Check your library and try again.",
            500,
        )
    }

    fn from_response(status: StatusCode, retry_after_seconds: Option<u64>) -> Self {
        let (code, message, http_status) = match status.as_u16() {
            401 | 403 => (
                "authentication_required",
                "Your Redrob API key was rejected. Reconnect this device with a valid key.",
                401,
            ),
            402 => (
                "insufficient_balance",
                "This workspace does not have enough balance for an answer.",
                402,
            ),
            429 => (
                "rate_limited",
                "Redrob is receiving too many requests. Wait a moment and try again.",
                429,
            ),
            500..=599 => (
                "service_unavailable",
                "Redrob is temporarily unavailable. Your local files were not affected.",
                503,
            ),
            _ => (
                "request_rejected",
                "Redrob could not accept this request. Review the connection settings and try again.",
                400,
            ),
        };
        Self {
            code: code.into(),
            message: message.into(),
            http_status,
            retry_after_seconds,
        }
    }
}

pub async fn ask(state: AppState, request: AskRequest) -> Result<AskResponse, RedrobError> {
    if !state.settings().send_context_to_redrob {
        return Err(RedrobError::new(
            "privacy_disabled",
            "Sending selected excerpts to Redrob is disabled in Privacy settings.",
            403,
        ));
    }
    let api_key = state.api_key().ok_or_else(|| {
        RedrobError::new(
            "not_connected",
            "Connect Redrob or paste an API key before asking.",
            401,
        )
    })?;
    let query = request.query.trim();
    if query.is_empty() || query.chars().count() > MAX_QUESTION_CHARACTERS {
        return Err(RedrobError::new(
            "invalid_request",
            "Enter a question between 1 and 4,000 characters.",
            400,
        ));
    }

    let max_sources = request.max_sources.clamp(1, 10);
    let chunks = if request.source_ids.is_empty() {
        let search_state = state.clone();
        let query = query.to_string();
        tauri::async_runtime::spawn_blocking(move || {
            search::search(
                &search_state,
                SearchRequest {
                    query,
                    limit: max_sources,
                    filters: SearchFilters::default(),
                },
            )
        })
        .await
        .map_err(RedrobError::local)?
        .map_err(RedrobError::local)?
        .into_iter()
        .map(|result| crate::models::ChunkRecord {
            id: result.chunk_id,
            document_id: result.document_id,
            chunk_index: 0,
            content: result.content,
            page: result.page,
            heading: result.heading,
            path: result.path,
            name: result.name,
            extension: result.extension,
            modified_at: result.modified_at,
        })
        .collect::<Vec<_>>()
    } else {
        state
            .storage()
            .get_chunks(&request.source_ids[..request.source_ids.len().min(max_sources)])
            .map_err(RedrobError::local)?
    };
    if chunks.is_empty() {
        return Err(RedrobError::new(
            "no_sources",
            "No relevant local sources were found for this question.",
            404,
        ));
    }

    let sources = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| AnswerSource {
            number: index + 1,
            chunk_id: chunk.id,
            path: chunk.path.clone(),
            name: chunk.name.clone(),
            page: chunk.page,
            excerpt: chunk.content.clone(),
        })
        .collect::<Vec<_>>();
    let mut remaining = MAX_CONTEXT_CHARACTERS;
    let context = sources
        .iter()
        .filter_map(|source| {
            if remaining == 0 {
                return None;
            }
            let excerpt = truncate_chars(&source.excerpt, remaining);
            remaining = remaining.saturating_sub(excerpt.chars().count());
            Some(format!("[{}]\n{excerpt}", source.number))
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let settings = state.settings();
    let body = ChatRequest {
        model: settings.answer_model.clone(),
        temperature: 0.2,
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: "Answer only from the supplied local sources. Cite factual statements with [1], [2], and so on. If the sources do not contain the answer, say so clearly. Reply in the language of the user's question.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!("LOCAL SOURCES\n{context}\n\nQUESTION\n{query}"),
            },
        ],
    };
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .user_agent(concat!("Redrob-Recall/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(RedrobError::local)?;
    let endpoint = format!(
        "{}/chat/completions",
        settings.redrob_base_url.trim_end_matches('/')
    );
    let started = std::time::Instant::now();
    let response = client
        .post(&endpoint)
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            tracing::warn!(%error, "could not reach Redrob");
            let (code, message) = if error.is_timeout() {
                (
                    "timeout",
                    "Redrob took too long to respond. Check your connection and try again.",
                )
            } else {
                (
                    "network_error",
                    "Redrob could not be reached. Check your connection and try again.",
                )
            };
            RedrobError::new(code, message, 503)
        })?;
    let status = response.status();
    let retry_after_seconds = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    if !status.is_success() {
        return Err(RedrobError::from_response(status, retry_after_seconds));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err(RedrobError::new(
            "invalid_response",
            "Redrob returned an unexpectedly large response.",
            502,
        ));
    }
    let mut raw = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            tracing::warn!(%error, "could not read Redrob response");
            RedrobError::new(
                "invalid_response",
                "The Redrob response could not be read. Try again.",
                502,
            )
        })?;
        if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(raw.len()) {
            return Err(RedrobError::new(
                "invalid_response",
                "Redrob returned an unexpectedly large response.",
                502,
            ));
        }
        raw.extend_from_slice(&chunk);
    }
    let completion: ChatResponse = serde_json::from_slice(&raw).map_err(|error| {
        tracing::warn!(%error, "Redrob returned an unexpected response shape");
        RedrobError::new(
            "invalid_response",
            "Redrob returned an unexpected response. Try again later.",
            502,
        )
    })?;
    let answer = completion
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .filter(|answer| !answer.trim().is_empty())
        .ok_or_else(|| {
            RedrobError::new(
                "empty_response",
                "Redrob returned no answer. Try a more specific question.",
                502,
            )
        })?;
    Ok(AskResponse {
        answer,
        sources,
        model: completion.model.unwrap_or(settings.answer_model),
        input_tokens: completion.usage.as_ref().map(|usage| usage.prompt_tokens),
        output_tokens: completion
            .usage
            .as_ref()
            .map(|usage| usage.completion_tokens),
        latency_ms: started.elapsed().as_millis() as u64,
    })
}

fn truncate_chars(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    temperature: f32,
    messages: Vec<ChatMessage>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
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
    content: String,
}

#[derive(Deserialize)]
struct ChatUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::{truncate_chars, RedrobError};
    use reqwest::StatusCode;

    #[test]
    fn excerpt_truncation_respects_unicode_characters() {
        assert_eq!(truncate_chars("가나다라마바사", 4), "가나다라");
    }

    #[test]
    fn upstream_errors_are_redacted_and_classified() {
        let error = RedrobError::from_response(StatusCode::UNAUTHORIZED, None);
        assert_eq!(error.code, "authentication_required");
        assert_eq!(error.http_status, 401);
        assert!(!error.message.contains("upstream"));
    }

    #[test]
    fn retry_after_is_preserved_for_rate_limits() {
        let error = RedrobError::from_response(StatusCode::TOO_MANY_REQUESTS, Some(30));
        assert_eq!(error.code, "rate_limited");
        assert_eq!(error.retry_after_seconds, Some(30));
    }
}
