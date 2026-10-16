// SPDX-License-Identifier: Apache-2.0
//! The typed client another Redrob app uses to reach a running Recall on the same machine.

use crate::paths::{ADDRESS_FILE, DEFAULT_PORT, TOKEN_FILE, TOKEN_PREFIX, default_data_dir};
use crate::types::*;
use serde::de::DeserializeOwned;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// No data directory, no token file, or a token that is not one of ours: Recall has never run
    /// on this machine (or runs with its API switched off).
    #[error("Redrob Recall is not running on this machine (no local API token found)")]
    NotAvailable,
    #[error("the local API could not be reached: {0}")]
    Transport(#[from] reqwest::Error),
    /// The server answered with an error body; `status` is the HTTP status.
    #[error("{message}")]
    Api {
        status: u16,
        message: String,
        code: Option<String>,
    },
}

/// A client bound to one server address and token. Clone is cheap.
#[derive(Clone, Debug)]
pub struct RecallClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

impl RecallClient {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: token.into(),
            http,
        }
    }

    /// Find the server through the files it leaves in the data directory: the token, and the
    /// address it bound (falling back to the default port when that file is missing).
    pub fn discover() -> Result<Self, ClientError> {
        let dir = default_data_dir().ok_or(ClientError::NotAvailable)?;
        let token = std::fs::read_to_string(dir.join(TOKEN_FILE))
            .map(|t| t.trim().to_string())
            .map_err(|_| ClientError::NotAvailable)?;
        if !token.starts_with(TOKEN_PREFIX) {
            return Err(ClientError::NotAvailable);
        }
        let base_url = std::fs::read_to_string(dir.join(ADDRESS_FILE))
            .map(|a| a.trim().to_string())
            .ok()
            .filter(|a| a.starts_with("http://127.0.0.1:"))
            .unwrap_or_else(|| format!("http://127.0.0.1:{DEFAULT_PORT}"));
        Ok(Self::new(base_url, token))
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn health(&self) -> Result<Health, ClientError> {
        self.get("/health").await
    }

    pub async fn status(&self) -> Result<Status, ClientError> {
        self.get("/v1/status").await
    }

    pub async fn search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>, ClientError> {
        let response: SearchResponse = self.post("/v1/search", request).await?;
        Ok(response.results)
    }

    pub async fn ask_context(&self, request: &AskRequest) -> Result<AskContext, ClientError> {
        self.post("/v1/ask-context", request).await
    }

    pub async fn add_folder(&self, path: &str) -> Result<Vec<FolderStatus>, ClientError> {
        let response: FoldersResponse = self
            .post("/v1/folders", &FolderRequest { path: path.into() })
            .await?;
        Ok(response.folders)
    }

    pub async fn remove_folder(&self, path: &str) -> Result<Vec<FolderStatus>, ClientError> {
        let response = self
            .http
            .delete(format!("{}/v1/folders", self.base_url))
            .bearer_auth(&self.token)
            .json(&FolderRequest { path: path.into() })
            .send()
            .await?;
        let folders: FoldersResponse = Self::read(response).await?;
        Ok(folders.folders)
    }

    pub async fn start_index(&self) -> Result<IndexResponse, ClientError> {
        self.post("/v1/index", &serde_json::json!({})).await
    }

    pub async fn pause_index(&self) -> Result<serde_json::Value, ClientError> {
        self.post("/v1/index/pause", &serde_json::json!({})).await
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Self::read(response).await
    }

    async fn post<T: DeserializeOwned, B: serde::Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let response = self
            .http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await?;
        Self::read(response).await
    }

    async fn read<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, ClientError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response.json().await?);
        }
        let body: ErrorBody = response.json().await.unwrap_or(ErrorBody {
            error: format!("the local API answered {status}"),
            code: None,
        });
        Err(ClientError::Api {
            status: status.as_u16(),
            message: body.error,
            code: body.code,
        })
    }
}
