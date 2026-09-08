use crate::{
    models::{AnswerSource, AskRequest, AskResponse, SearchFilters, SearchRequest},
    search,
    state::AppState,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;

pub async fn ask(state: AppState, request: AskRequest) -> Result<AskResponse> {
    anyhow::ensure!(
        state.settings().send_context_to_redrob,
        "sending selected excerpts to Redrob is disabled in Privacy settings"
    );
    let api_key = state
        .api_key()
        .context("connect Redrob or paste an API key before asking")?;
    let max_sources = request.max_sources.clamp(1, 10);
    let chunks = if request.source_ids.is_empty() {
        let search_state = state.clone();
        let query = request.query.clone();
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
        .await??
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
            .get_chunks(&request.source_ids[..request.source_ids.len().min(max_sources)])?
    };
    anyhow::ensure!(!chunks.is_empty(), "no relevant local sources were found");

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
    let context = sources
        .iter()
        .map(|source| format!("[{}]\n{}", source.number, source.excerpt))
        .collect::<Vec<_>>()
        .join("\n\n");
    let settings = state.settings();
    let user_message = format!("LOCAL SOURCES\n{context}\n\nQUESTION\n{}", request.query);
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
                content: user_message,
            },
        ],
    };
    let started = Instant::now();
    let response = reqwest::Client::new()
        .post(format!(
            "{}/chat/completions",
            settings.redrob_base_url.trim_end_matches('/')
        ))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .context("could not reach Redrob")?;
    let status = response.status();
    let raw = response.text().await?;
    anyhow::ensure!(status.is_success(), "Redrob returned {status}: {raw}");
    let completion: ChatResponse =
        serde_json::from_str(&raw).context("Redrob returned an unexpected response")?;
    let answer = completion
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .context("Redrob returned no answer")?;
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
