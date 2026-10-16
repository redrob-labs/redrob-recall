// SPDX-License-Identifier: Apache-2.0
//! The wire types, as a client sees them. They mirror the core crate's models field for field (the
//! server serialises those directly), so a client does not need to depend on the core crate and its
//! embedding model and vector index.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub status: String,
    pub product: String,
    pub version: String,
    pub documents: u64,
    pub chunks: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IndexStatus {
    #[default]
    Idle,
    Scanning,
    Indexing,
    Paused,
    Error,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub documents: u64,
    pub indexed_chunks: u64,
    pub pending: u64,
    pub failed: u64,
    pub total_bytes: u64,
    pub status: IndexStatus,
    pub current_file: Option<String>,
    pub last_indexed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FolderStatus {
    pub path: String,
    pub present: bool,
    pub files: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub processed: u64,
    pub total: u64,
    pub current_file: Option<String>,
    pub status: IndexStatus,
    pub message: String,
}

/// `GET /v1/status`: the settings are passed through as JSON (their shape is the core crate's
/// `AppSettings`; a client rarely needs more than `libraryPaths`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub settings: serde_json::Value,
    pub stats: IndexStats,
    pub data_directory: String,
    pub version: String,
    pub folders: Vec<FolderStatus>,
    #[serde(default)]
    pub progress: Option<IndexProgress>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilters {
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    #[serde(default)]
    pub filters: SearchFilters,
}

fn default_search_limit() -> usize {
    12
}

impl SearchRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            limit: default_search_limit(),
            filters: SearchFilters::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub chunk_id: u64,
    pub document_id: i64,
    pub path: String,
    pub name: String,
    pub extension: String,
    pub page: Option<u32>,
    pub heading: Option<String>,
    pub content: String,
    #[serde(default)]
    pub snippet: Option<String>,
    pub score: f32,
    pub vector_score: f32,
    pub keyword_score: f32,
    #[serde(default)]
    pub document_name_score: f32,
    pub modified_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskRequest {
    pub query: String,
    #[serde(default)]
    pub source_ids: Vec<u64>,
    #[serde(default = "default_source_limit")]
    pub max_sources: usize,
}

fn default_source_limit() -> usize {
    6
}

impl AskRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            source_ids: Vec::new(),
            max_sources: default_source_limit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSource {
    pub number: usize,
    pub chunk_id: u64,
    pub path: String,
    pub name: String,
    pub page: Option<u32>,
    pub excerpt: String,
}

/// `POST /v1/ask-context`: the numbered evidence and the two prompts to send to a chat model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskContext {
    pub query: String,
    pub sources: Vec<AnswerSource>,
    pub context: String,
    pub system_prompt: String,
    pub user_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldersResponse {
    pub folders: Vec<FolderStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexResponse {
    pub started: bool,
    pub queued: bool,
}

/// Every error body: `{"error": "<sentence>", "code": "<optional stable code>"}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub error: String,
    #[serde(default)]
    pub code: Option<String>,
}
