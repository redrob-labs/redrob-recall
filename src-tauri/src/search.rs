use crate::{
    models::{SearchRequest, SearchResult, VECTOR_NAME},
    state::AppState,
};
use anyhow::Result;
use qdrant_edge::{NamedQuery, QueryEnum, QueryRequestBuilder, ScoringQuery, WithPayloadInterface};
use std::collections::HashMap;

pub fn search(state: &AppState, request: SearchRequest) -> Result<Vec<SearchResult>> {
    let query = request.query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let limit = request.limit.clamp(1, 50);
    let candidate_limit = (limit * 4).clamp(20, 160);
    let keyword = state.storage().keyword_search(query, candidate_limit)?;
    let vector = vector_search(state, query, candidate_limit).unwrap_or_else(|error| {
        tracing::warn!(%error, "semantic search unavailable; using keyword results only");
        Vec::new()
    });

    let mut fused: HashMap<u64, FusionScore> = HashMap::new();
    for (rank, (id, raw_score)) in vector.iter().enumerate() {
        let score = fused.entry(*id).or_default();
        score.combined += 1.0 / (60.0 + rank as f32 + 1.0);
        score.vector = *raw_score;
    }
    for (rank, hit) in keyword.iter().enumerate() {
        let score = fused.entry(hit.chunk_id).or_default();
        score.combined += 1.0 / (60.0 + rank as f32 + 1.0);
        score.keyword = hit.score;
        if !hit.snippet.is_empty() {
            score.snippet = Some(hit.snippet.clone());
        }
    }

    // A third arm: documents whose NAME or PATH matched. Represented by each document's first chunk,
    // because a name match carries no in-chunk evidence to point at -- and returning every chunk of a
    // matching document would bury the other arms under forty copies of one result.
    //
    // Fused with the same reciprocal rank as the others rather than given its own weighting. A filename
    // match is strong evidence about relevance and weak evidence about which passage answers the
    // question, and RRF already expresses that: it contributes one good rank, not a high score.
    let documents = state
        .storage()
        .document_search(query, candidate_limit)
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "document name search unavailable; using the other arms");
            Vec::new()
        });
    if !documents.is_empty() {
        let ids: Vec<i64> = documents.iter().map(|hit| hit.document_id).collect();
        let first_chunks = state
            .storage()
            .first_chunks_of(&ids)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not resolve documents to chunks");
                Vec::new()
            });
        let by_document: HashMap<i64, u64> = first_chunks.into_iter().collect();
        for (rank, hit) in documents.iter().enumerate() {
            let Some(chunk_id) = by_document.get(&hit.document_id) else {
                // A document with no chunks yet -- indexed but not chunked, or emptied. Skipped rather
                // than surfaced as a result with nothing to show.
                continue;
            };
            let score = fused.entry(*chunk_id).or_default();
            score.combined += 1.0 / (60.0 + rank as f32 + 1.0);
            score.document_name = hit.score;
            // Only when the other arms found nothing to quote: a passage containing the search term is
            // more useful to read than the filename that also contains it.
            if score.snippet.is_none() && !hit.snippet.is_empty() {
                score.snippet = Some(hit.snippet.clone());
            }
        }
    }

    let mut ranked = fused.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.combined.total_cmp(&left.1.combined));
    let mut output = Vec::with_capacity(limit);
    for (id, fusion) in ranked {
        let Some(chunk) = state.storage().get_chunk(id)? else {
            continue;
        };
        if !request.filters.extensions.is_empty()
            && !request
                .filters
                .extensions
                .iter()
                .any(|extension| extension.eq_ignore_ascii_case(&chunk.extension))
        {
            continue;
        }
        if let Some(prefix) = request.filters.path_prefix.as_deref() {
            if !chunk.path.starts_with(prefix) {
                continue;
            }
        }
        output.push(SearchResult {
            chunk_id: chunk.id,
            document_id: chunk.document_id,
            path: chunk.path,
            name: chunk.name,
            extension: chunk.extension,
            page: chunk.page,
            heading: chunk.heading,
            content: chunk.content,
            snippet: fusion.snippet.clone(),
            score: fusion.combined,
            vector_score: fusion.vector,
            keyword_score: fusion.keyword,
            document_name_score: fusion.document_name,
            modified_at: chunk.modified_at,
        });
        if output.len() >= limit {
            break;
        }
    }
    normalize_scores(&mut output);
    Ok(output)
}

fn vector_search(state: &AppState, query: &str, limit: usize) -> Result<Vec<(u64, f32)>> {
    let embedding = state.embedder().embed_query(query)?;
    state.with_shard(|shard| {
        let results = shard.query(
            QueryRequestBuilder::new(limit)
                .query(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery {
                    query: embedding.into(),
                    using: Some(VECTOR_NAME.to_string()),
                })))
                .with_payload(WithPayloadInterface::Bool(false))
                .build(),
        )?;
        Ok(results
            .into_iter()
            .filter_map(|point| {
                point
                    .id
                    .to_string()
                    .parse::<u64>()
                    .ok()
                    .map(|id| (id, point.score))
            })
            .collect())
    })
}

fn normalize_scores(results: &mut [SearchResult]) {
    let max = results
        .iter()
        .map(|result| result.score)
        .fold(0.0_f32, f32::max);
    if max > 0.0 {
        for result in results {
            result.score = (result.score / max).clamp(0.0, 1.0);
        }
    }
}

#[derive(Default)]
struct FusionScore {
    combined: f32,
    vector: f32,
    keyword: f32,
    /// How well this chunk's DOCUMENT matched by name or path, 0 when it did not.
    document_name: f32,
    /// The window FTS5 chose around the match, when the keyword half found this chunk.
    ///
    /// Absent for a vector-only hit, which has no single matching term to centre on -- the result falls
    /// back to the head of the chunk there, and that is honest rather than a gap: a semantic match is a
    /// whole-passage judgement, so there is nothing to point at.
    snippet: Option<String>,
}
