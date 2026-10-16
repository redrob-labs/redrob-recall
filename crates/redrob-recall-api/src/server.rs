// SPDX-License-Identifier: Apache-2.0
//! The loopback HTTP API: 127.0.0.1 only, one bearer token, JSON in and out. Promoted from the Tauri
//! app's `local_api.rs` and widened from search/ask to the whole memory layer: another Redrob app can
//! add a folder, read the index status, search, and fetch the numbered evidence for a question
//! (`ask-context`) to send to whatever model it talks to. docs/API.md is the contract.
//!
//! The token lives in `<data dir>/local-api-token` (0600), created on first start; the bound address
//! is written to `<data dir>/local-api-address` so a client on the same machine finds the server
//! without configuration ([`crate::client::RecallClient::discover`]).

use crate::paths::{ADDRESS_FILE, TOKEN_FILE, TOKEN_PREFIX};
use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::IntoResponse,
    routing::{get, post},
};
use redrob_recall_core::{AskRequest, Library, SearchRequest, ask_context, search};
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Clone)]
struct ApiContext {
    library: Library,
    token: Arc<String>,
    search_limit: Arc<Semaphore>,
}

/// Serve the API for `library` on `127.0.0.1:<settings.local_api_port>` until the task is dropped.
/// Returns at once, doing nothing, when the settings have the API switched off.
pub async fn serve(library: Library) -> Result<()> {
    let settings = library.settings();
    if !settings.local_api_enabled {
        return Ok(());
    }
    serve_on(library, settings.local_api_port).await
}

/// [`serve`] on an explicit port (the CLI's `serve --port`).
pub async fn serve_on(library: Library, port: u16) -> Result<()> {
    let token = Arc::new(load_or_create_token(&library)?);
    let context = ApiContext {
        library: library.clone(),
        token,
        search_limit: Arc::new(Semaphore::new(4)),
    };
    let router = router(context);
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("could not bind local API at {address}"))?;
    let bound = listener.local_addr().unwrap_or(address);
    write_address(&library, &bound)?;
    tracing::info!(%bound, "local retrieval API listening");
    axum::serve(listener, router).await?;
    Ok(())
}

fn router(context: ApiContext) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/status", get(status))
        .route("/v1/search", post(search_handler))
        .route("/v1/ask-context", post(ask_context_handler))
        .route("/v1/folders", post(add_folder).delete(remove_folder))
        .route("/v1/index", post(start_index))
        .route("/v1/index/pause", post(pause_index))
        .with_state(context)
}

async fn health(State(context): State<ApiContext>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(&headers, &context.token) {
        return api_error(StatusCode::UNAUTHORIZED, "invalid local API token");
    }
    match context.library.snapshot() {
        Ok(snapshot) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "product": "Redrob Recall",
                "version": snapshot.version,
                "documents": snapshot.stats.documents,
                "chunks": snapshot.stats.indexed_chunks,
            })),
        ),
        Err(error) => {
            tracing::error!(%error, "local API health check failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "local service health check failed",
            )
        }
    }
}

/// The whole snapshot: settings, counts, folders, the running pass's progress.
async fn status(State(context): State<ApiContext>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(&headers, &context.token) {
        return api_error(StatusCode::UNAUTHORIZED, "invalid local API token");
    }
    match context.library.snapshot() {
        Ok(snapshot) => {
            let mut value = serde_json::to_value(&snapshot).unwrap_or_default();
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "progress".into(),
                    serde_json::to_value(context.library.progress()).unwrap_or_default(),
                );
            }
            (StatusCode::OK, Json(value))
        }
        Err(error) => {
            tracing::error!(%error, "local API status failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "status could not be read",
            )
        }
    }
}

async fn search_handler(
    State(context): State<ApiContext>,
    headers: HeaderMap,
    Json(request): Json<SearchRequest>,
) -> impl IntoResponse {
    if !authorized(&headers, &context.token) {
        return api_error(StatusCode::UNAUTHORIZED, "invalid local API token");
    }
    let Ok(permit) = context.search_limit.clone().try_acquire_owned() else {
        return api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "too many search requests are already running",
        );
    };
    let library = context.library.clone();
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        search(&library, request)
    })
    .await
    {
        Ok(Ok(results)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "results": results })),
        ),
        Ok(Err(error)) => {
            tracing::warn!(%error, "local API search rejected");
            api_error(
                StatusCode::BAD_REQUEST,
                "search request could not be completed",
            )
        }
        Err(error) => {
            tracing::error!(%error, "local API search worker failed");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "search service failed")
        }
    }
}

/// The numbered evidence and the prompts for a question; the caller sends them to its model.
async fn ask_context_handler(
    State(context): State<ApiContext>,
    headers: HeaderMap,
    Json(request): Json<AskRequest>,
) -> impl IntoResponse {
    if !authorized(&headers, &context.token) {
        return api_error(StatusCode::UNAUTHORIZED, "invalid local API token");
    }
    let Ok(permit) = context.search_limit.clone().try_acquire_owned() else {
        return api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "too many requests are already running",
        );
    };
    let library = context.library.clone();
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        ask_context(&library, &request)
    })
    .await
    {
        Ok(Ok(evidence)) => (StatusCode::OK, Json(serde_json::json!(evidence))),
        Ok(Err(error)) => (
            StatusCode::from_u16(error.http_status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(serde_json::json!({ "error": error.message, "code": error.code })),
        ),
        Err(error) => {
            tracing::error!(%error, "local API ask-context worker failed");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "ask service failed")
        }
    }
}

#[derive(Deserialize)]
struct FolderRequest {
    path: String,
}

async fn add_folder(
    State(context): State<ApiContext>,
    headers: HeaderMap,
    Json(request): Json<FolderRequest>,
) -> impl IntoResponse {
    if !authorized(&headers, &context.token) {
        return api_error(StatusCode::UNAUTHORIZED, "invalid local API token");
    }
    match context.library.add_folder(&request.path) {
        Ok(()) => folders_response(&context),
        Err(message) => api_error(StatusCode::BAD_REQUEST, &message),
    }
}

async fn remove_folder(
    State(context): State<ApiContext>,
    headers: HeaderMap,
    Json(request): Json<FolderRequest>,
) -> impl IntoResponse {
    if !authorized(&headers, &context.token) {
        return api_error(StatusCode::UNAUTHORIZED, "invalid local API token");
    }
    match context.library.remove_folder(&request.path) {
        Ok(()) => folders_response(&context),
        Err(message) => api_error(StatusCode::CONFLICT, &message),
    }
}

fn folders_response(context: &ApiContext) -> (StatusCode, Json<serde_json::Value>) {
    match context.library.folders() {
        Ok(folders) => (
            StatusCode::OK,
            Json(serde_json::json!({ "folders": folders })),
        ),
        Err(error) => {
            tracing::error!(%error, "local API could not list folders");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "folders could not be read",
            )
        }
    }
}

/// Start a pass now, or queue one behind the running pass. `started` says which.
async fn start_index(State(context): State<ApiContext>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(&headers, &context.token) {
        return api_error(StatusCode::UNAUTHORIZED, "invalid local API token");
    }
    let started = context.library.request_index();
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "started": started, "queued": !started })),
    )
}

async fn pause_index(State(context): State<ApiContext>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(&headers, &context.token) {
        return api_error(StatusCode::UNAUTHORIZED, "invalid local API token");
    }
    context.library.pause_indexing();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": context.library.status() })),
    )
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|value| value == token)
}

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": message })))
}

/// The token every caller must present, read from `<data dir>/local-api-token` or created there.
pub fn load_or_create_token(library: &Library) -> Result<String> {
    let path = library.data_dir().join(TOKEN_FILE);
    if path.exists() {
        let token = std::fs::read_to_string(&path)?.trim().to_string();
        anyhow::ensure!(
            token.starts_with(TOKEN_PREFIX) && token.len() >= 40,
            "local API token file is invalid"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        return Ok(token);
    }
    let token = format!("{TOKEN_PREFIX}{}", Uuid::new_v4().simple());
    let temporary = library
        .data_dir()
        .join(format!(".local-api-token-{}", Uuid::new_v4().simple()));
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(token.as_bytes())?;
        file.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&temporary, &path)?;
    Ok(token)
}

fn write_address(library: &Library, address: &SocketAddr) -> Result<()> {
    let path = library.data_dir().join(ADDRESS_FILE);
    std::fs::write(&path, format!("http://{address}\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
