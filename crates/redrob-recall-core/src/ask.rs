// SPDX-License-Identifier: Apache-2.0
//! Ask: the local half. A question becomes numbered evidence (the best chunks, with the keyword
//! snippet centred on the match when there is one) and a prompt a chat model answers from, with
//! `[1]`, `[2]` citations pointing back at the sources. Promoted from the Tauri app's `redrob.rs`;
//! the model call itself is the caller's -- the desktop shell sends the prompt to the Redrob console
//! with the shared account, another app may send it anywhere -- which is why the loopback API serves
//! `ask-context` and not an answer.

use crate::{
    library::Library,
    models::{AnswerSource, AskRequest, ChunkRecord, SearchFilters, SearchRequest},
    search,
};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_QUESTION_CHARACTERS: usize = 4_000;
pub const MAX_CONTEXT_CHARACTERS: usize = 60_000;

/// The instruction every Ask sends with the evidence.
pub const SYSTEM_PROMPT: &str = "Answer only from the supplied local sources. Cite factual statements with [1], [2], and so on. If the sources do not contain the answer, say so clearly. Reply in the language of the user's question.";

/// Why the evidence could not be prepared. `code` is stable and machine-readable; `message` is a
/// sentence for a person; `http_status` is what the loopback API answers with.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskError {
    pub code: String,
    pub message: String,
    pub http_status: u16,
}

impl fmt::Display for AskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AskError {}

impl AskError {
    pub fn new(code: &str, message: impl Into<String>, http_status: u16) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            http_status,
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
}

/// The evidence for one question, ready to send to a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskContext {
    /// The question, trimmed.
    pub query: String,
    /// The numbered sources, in citation order.
    pub sources: Vec<AnswerSource>,
    /// The sources as the model sees them: `[n]` then the excerpt, within the context budget.
    pub context: String,
    /// [`SYSTEM_PROMPT`].
    pub system_prompt: String,
    /// `LOCAL SOURCES ... QUESTION ...`: the user turn.
    pub user_prompt: String,
}

/// Gather the evidence for `request`: a search when no source ids are given, the named chunks
/// otherwise. Blocking (the vector search embeds the question).
pub fn ask_context(library: &Library, request: &AskRequest) -> Result<AskContext, AskError> {
    let query = request.query.trim();
    if query.is_empty() || query.chars().count() > MAX_QUESTION_CHARACTERS {
        return Err(AskError::new(
            "invalid_request",
            "Enter a question between 1 and 4,000 characters.",
            400,
        ));
    }

    let max_sources = request.max_sources.clamp(1, 10);
    let chunks: Vec<(ChunkRecord, Option<String>)> = if request.source_ids.is_empty() {
        search::search(
            library,
            SearchRequest {
                query: query.to_string(),
                limit: max_sources,
                filters: SearchFilters::default(),
            },
        )
        .map_err(AskError::local)?
        .into_iter()
        .map(|result| {
            // The snippet travels beside the chunk rather than inside ChunkRecord, because that type
            // is also produced by get_chunks below, where no search ran and there is nothing to centre
            // on.
            let snippet = result.snippet.clone();
            (
                ChunkRecord {
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
                },
                snippet,
            )
        })
        .collect()
    } else {
        library
            .storage()
            .get_chunks(&request.source_ids[..request.source_ids.len().min(max_sources)])
            .map_err(AskError::local)?
            .into_iter()
            .map(|chunk| (chunk, None))
            .collect()
    };
    if chunks.is_empty() {
        return Err(AskError::new(
            "no_sources",
            "No relevant local sources were found for this question.",
            404,
        ));
    }

    let sources = chunks
        .iter()
        .enumerate()
        .map(|(index, (chunk, snippet))| AnswerSource {
            number: index + 1,
            chunk_id: chunk.id,
            path: chunk.path.clone(),
            name: chunk.name.clone(),
            page: chunk.page,
            // The snippet when the search produced one, the whole chunk otherwise. The context budget
            // below truncates from the excerpt's FIRST character, so a whole chunk whose match sat at
            // character 824 handed the model 120 characters that did not contain the searched term.
            excerpt: snippet.clone().unwrap_or_else(|| chunk.content.clone()),
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

    Ok(AskContext {
        user_prompt: format!("LOCAL SOURCES\n{context}\n\nQUESTION\n{query}"),
        query: query.to_string(),
        sources,
        context,
        system_prompt: SYSTEM_PROMPT.to_string(),
    })
}

pub fn truncate_chars(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::truncate_chars;

    #[test]
    fn excerpt_truncation_respects_unicode_characters() {
        assert_eq!(truncate_chars("가나다라마바사", 4), "가나다라");
    }
}
