use crate::{
    indexer,
    models::{
        AppSettings, AppSnapshot, AskRequest, AskResponse, ChunkRecord, ConnectionStatus,
        DocumentRecord, SearchRequest, SearchResult,
    },
    redrob, search,
    state::AppState,
};
use std::path::PathBuf;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

fn command_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    state.snapshot().map_err(command_error)
}

#[tauri::command]
pub async fn choose_library_folder(app: AppHandle) -> Result<Option<String>, String> {
    let selection = app.dialog().file().blocking_pick_folder();
    Ok(selection.map(|path| path.to_string()))
}

#[tauri::command]
pub fn add_library_path(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let canonical = std::fs::canonicalize(&path).map_err(command_error)?;
    if !canonical.is_dir() {
        return Err("Choose a folder, not a file".into());
    }
    let mut settings = state.settings();
    let path = canonical.to_string_lossy().to_string();
    if !settings.library_paths.contains(&path) {
        settings.library_paths.push(path);
        settings.library_paths.sort();
    }
    state.update_settings(settings).map_err(command_error)
}

#[tauri::command]
pub fn remove_library_path(path: String, state: State<'_, AppState>) -> Result<(), String> {
    if !state.set_indexing(true) {
        return Err("Wait for indexing to finish before removing a library folder".into());
    }
    let mut settings = state.settings();
    settings
        .library_paths
        .retain(|candidate| candidate != &path);
    if let Err(error) = state.persist_settings(settings) {
        state.set_indexing(false);
        state.emit_snapshot();
        return Err(command_error(error));
    }
    let watcher_result = indexer::configure_watcher(state.inner());
    state.emit_snapshot();
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = indexer::start_claimed_full_index(state).await {
            tracing::error!(%error, "library reconciliation failed after removing a folder");
        }
    });
    watcher_result.map_err(command_error)
}

#[tauri::command]
pub fn start_indexing(state: State<'_, AppState>) -> Result<bool, String> {
    if state.is_indexing() {
        return Ok(false);
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = indexer::start_full_index(state.clone()).await {
            tracing::error!(%error, "indexing failed");
            state.set_indexing(false);
            state.emit_snapshot();
        }
    });
    Ok(true)
}

#[tauri::command]
pub fn pause_indexing(state: State<'_, AppState>) {
    state.set_paused(true);
}

#[tauri::command]
pub async fn search_library(
    request: SearchRequest,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || search::search(&state, request))
        .await
        .map_err(command_error)?
        .map_err(command_error)
}

#[tauri::command]
pub async fn ask_library(
    request: AskRequest,
    state: State<'_, AppState>,
) -> Result<AskResponse, redrob::RedrobError> {
    redrob::ask(state.inner().clone(), request).await
}

#[tauri::command]
pub fn get_documents(
    limit: Option<usize>,
    offset: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<DocumentRecord>, String> {
    state
        .storage()
        .list_documents(limit.unwrap_or(100).clamp(1, 500), offset.unwrap_or(0))
        .map_err(command_error)
}

#[tauri::command]
pub fn get_sources(ids: Vec<u64>, state: State<'_, AppState>) -> Result<Vec<ChunkRecord>, String> {
    state.storage().get_chunks(&ids).map_err(command_error)
}

#[tauri::command]
pub fn open_source(path: String, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let source = validated_source(&path, &state)?;
    app.opener()
        .open_path(source.to_string_lossy(), None::<&str>)
        .map_err(command_error)
}

#[tauri::command]
pub fn reveal_source(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let source = validated_source(&path, &state)?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(format!("/select,{}", source.display()))
        .spawn()
        .map_err(command_error)?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("-R")
        .arg(&source)
        .spawn()
        .map_err(command_error)?;
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(source.parent().unwrap_or(&source))
        .spawn()
        .map_err(command_error)?;
    Ok(())
}

#[tauri::command]
pub fn save_settings(settings: AppSettings, state: State<'_, AppState>) -> Result<(), String> {
    state.update_settings(settings).map_err(command_error)
}

#[tauri::command]
pub fn connect_with_api_key(
    api_key: String,
    state: State<'_, AppState>,
) -> Result<ConnectionStatus, String> {
    state.set_api_key(api_key).map_err(command_error)?;
    Ok(state.connection_status())
}

#[tauri::command]
pub fn disconnect_redrob(state: State<'_, AppState>) -> ConnectionStatus {
    state.disconnect();
    state.connection_status()
}

#[tauri::command]
pub fn get_connection_status(state: State<'_, AppState>) -> ConnectionStatus {
    state.connection_status()
}

#[tauri::command]
pub fn create_library_backup(state: State<'_, AppState>) -> Result<String, String> {
    if state.is_indexing() {
        return Err("Pause indexing before creating a backup".into());
    }
    state.storage().integrity_check().map_err(command_error)?;
    state
        .storage()
        .create_backup()
        .map(|path| path.to_string_lossy().to_string())
        .map_err(command_error)
}

#[tauri::command]
pub fn check_library_health(state: State<'_, AppState>) -> Result<String, String> {
    state.storage().integrity_check().map_err(command_error)?;
    Ok("Local metadata and document references passed their integrity checks".into())
}

fn validated_source(path: &str, state: &AppState) -> Result<PathBuf, String> {
    let source = std::fs::canonicalize(path).map_err(|_| "That source file no longer exists")?;
    if !source.is_file() {
        return Err("That source is not a file".into());
    }
    let allowed = state.settings().library_paths.iter().any(|root| {
        std::fs::canonicalize(root).is_ok_and(|canonical_root| source.starts_with(canonical_root))
    });
    if !allowed {
        return Err("That file is outside your selected library folders".into());
    }
    Ok(source)
}

#[tauri::command]
pub fn clear_library(state: State<'_, AppState>) -> Result<(), String> {
    if state.is_indexing() {
        return Err("Pause indexing before clearing the library".into());
    }
    state.storage().clear().map_err(command_error)?;
    state.reset_shard().map_err(command_error)?;
    state.emit_snapshot();
    Ok(())
}
