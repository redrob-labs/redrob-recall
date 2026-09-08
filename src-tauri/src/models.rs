use serde::{Deserialize, Serialize};

pub const EMBEDDING_DIMENSION: usize = 384;
pub const VECTOR_NAME: &str = "semantic";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
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

impl AppSettings {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=250).contains(&self.max_file_size_mb),
            "maximum file size must be between 1 MB and 250 MB"
        );
        anyhow::ensure!(
            (256..=8_192).contains(&self.chunk_size),
            "passage size must be between 256 and 8192 characters"
        );
        anyhow::ensure!(
            self.chunk_overlap < self.chunk_size && self.chunk_overlap <= 2_048,
            "passage overlap must be smaller than the passage size"
        );
        anyhow::ensure!(
            !self.answer_model.trim().is_empty() && self.answer_model.len() <= 128,
            "answer model is invalid"
        );
        anyhow::ensure!(
            (1_024..=65_535).contains(&self.local_api_port),
            "local API port must be between 1024 and 65535"
        );
        anyhow::ensure!(
            (1..=32).contains(&self.include_extensions.len()),
            "choose between 1 and 32 supported file types"
        );
        anyhow::ensure!(
            self.include_extensions.iter().all(|extension| {
                !extension.is_empty()
                    && extension.len() <= 16
                    && extension
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric())
            }),
            "supported file types contain an invalid extension"
        );
        let endpoint = url::Url::parse(self.redrob_base_url.trim())
            .map_err(|_| anyhow::anyhow!("Redrob API URL is invalid"))?;
        let local_http = endpoint.scheme() == "http"
            && endpoint
                .host_str()
                .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
        anyhow::ensure!(
            endpoint.scheme() == "https" || local_http,
            "Redrob API URL must use HTTPS (HTTP is allowed only for localhost)"
        );
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::AppSettings;

    #[test]
    fn default_settings_are_valid() {
        AppSettings::default().validate().unwrap();
    }

    #[test]
    fn unsafe_remote_http_endpoint_is_rejected() {
        let settings = AppSettings {
            redrob_base_url: "http://example.com/v1".into(),
            ..AppSettings::default()
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn localhost_http_endpoint_is_allowed_for_development() {
        let settings = AppSettings {
            redrob_base_url: "http://127.0.0.1:8080/v1".into(),
            ..AppSettings::default()
        };
        settings.validate().unwrap();
    }
}
