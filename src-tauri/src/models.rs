use serde::{Deserialize, Serialize};

pub const EMBEDDING_DIMENSION: usize = 384;
pub const VECTOR_NAME: &str = "semantic";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub library_paths: Vec<String>,
    pub excluded_paths: Vec<String>,
    pub include_extensions: Vec<String>,
    pub max_file_size_mb: u64,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub answer_model: String,
    pub send_context_to_redrob: bool,
    pub local_api_enabled: bool,
    pub local_api_port: u16,
    pub redrob_base_url: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            library_paths: Vec::new(),
            excluded_paths: vec![
                ".git".into(),
                "node_modules".into(),
                ".cache".into(),
                "target".into(),
                "dist".into(),
            ],
            include_extensions: vec![
                "pdf", "docx", "txt", "md", "markdown", "rst", "csv", "json", "html", "htm", "log",
                "xml",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            max_file_size_mb: 50,
            chunk_size: 1_200,
            chunk_overlap: 180,
            answer_model: "auto".into(),
            send_context_to_redrob: true,
            local_api_enabled: true,
            local_api_port: 47331,
            redrob_base_url: "https://console.redrob.ai/api/backend/v1".into(),
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub connected: bool,
    pub key_label: Option<String>,
    pub endpoint: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub settings: AppSettings,
    pub stats: IndexStats,
    pub connection: ConnectionStatus,
    pub data_directory: String,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRecord {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub extension: String,
    pub mime_type: String,
    pub modified_at: String,
    pub size_bytes: u64,
    pub status: String,
    pub chunk_count: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChunkInput {
    pub content: String,
    pub page: Option<u32>,
    pub heading: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkRecord {
    pub id: u64,
    pub document_id: i64,
    pub chunk_index: u32,
    pub content: String,
    pub page: Option<u32>,
    pub heading: Option<String>,
    pub path: String,
    pub name: String,
    pub extension: String,
    pub modified_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilters {
    pub extensions: Vec<String>,
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
    pub score: f32,
    pub vector_score: f32,
    pub keyword_score: f32,
    pub modified_at: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskResponse {
    pub answer: String,
    pub sources: Vec<AnswerSource>,
    pub model: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub latency_ms: u64,
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
