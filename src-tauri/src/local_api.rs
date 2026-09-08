use crate::{
    models::{AskRequest, SearchRequest},
    redrob, search,
    state::AppState,
};
use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::{net::SocketAddr, sync::Arc};
use uuid::Uuid;

#[derive(Clone)]
struct ApiContext {
    state: AppState,
    token: Arc<String>,
}

pub async fn serve(state: AppState) -> Result<()> {
    let settings = state.settings();
    if !settings.local_api_enabled {
        return Ok(());
    }
    let token = Arc::new(load_or_create_token(&state)?);
    let context = ApiContext {
        state: state.clone(),
        token,
    };
    let router = Router::new()
        .route("/health", get(health))
        .route("/v1/search", post(search_handler))
        .route("/v1/ask", post(ask_handler))
        .with_state(context);
    let address = SocketAddr::from(([127, 0, 0, 1], settings.local_api_port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("could not bind local API at {address}"))?;
    tracing::info!(%address, "local retrieval API listening");
    axum::serve(listener, router).await?;
    Ok(())
}

async fn health(State(context): State<ApiContext>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(&headers, &context.token) {
        return api_error(StatusCode::UNAUTHORIZED, "invalid local API token");
    }
    let snapshot = context.state.snapshot();
    match snapshot {
        Ok(snapshot) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "product": "Redrob VectorDB",
                "version": snapshot.app_version,
                "documents": snapshot.stats.documents,
                "chunks": snapshot.stats.indexed_chunks,
            })),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"status": "error", "message": error.to_string()})),
        ),
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
    let state = context.state.clone();
    match tokio::task::spawn_blocking(move || search::search(&state, request)).await {
        Ok(Ok(results)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "results": results })),
        ),
        Ok(Err(error)) => api_error(StatusCode::BAD_REQUEST, &error.to_string()),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn ask_handler(
    State(context): State<ApiContext>,
    headers: HeaderMap,
    Json(request): Json<AskRequest>,
) -> impl IntoResponse {
    if !authorized(&headers, &context.token) {
        return api_error(StatusCode::UNAUTHORIZED, "invalid local API token");
    }
    match redrob::ask(context.state, request).await {
        Ok(answer) => (StatusCode::OK, Json(serde_json::json!(answer))),
        Err(error) => api_error(StatusCode::BAD_REQUEST, &error.to_string()),
    }
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

fn load_or_create_token(state: &AppState) -> Result<String> {
    let path = state.data_dir().join("local-api-token");
    if path.exists() {
        return Ok(std::fs::read_to_string(path)?.trim().to_string());
    }
    let token = format!("rrv_local_{}", Uuid::new_v4().simple());
    std::fs::write(&path, &token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(token)
}

#[derive(Serialize)]
struct _SchemaMarker;
