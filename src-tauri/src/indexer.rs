use crate::{
    models::{ChunkInput, IndexProgress, IndexStatus, VECTOR_NAME},
    state::AppState,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use notify::{RecursiveMode, Watcher};
use qdrant_edge::{
    PointInsertOperations, PointOperations, PointStruct, PointStructPersisted, UpdateOperation,
    Vectors,
};
use quick_xml::{events::Event, Reader};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use tauri::Emitter;
use zip::ZipArchive;

pub fn configure_watcher(state: &AppState) -> Result<()> {
    let state_for_callback = state.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let Ok(event) = result else { return };
        if !event.kind.is_create() && !event.kind.is_modify() && !event.kind.is_remove() {
            return;
        }
        if state_for_callback.is_indexing() || state_for_callback.is_paused() {
            return;
        }
        let state = state_for_callback.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(900)).await;
            let _ = start_full_index(state).await;
        });
    })?;
    for library_path in state.settings().library_paths {
        let path = PathBuf::from(&library_path);
        if path.exists() {
            watcher.watch(&path, RecursiveMode::Recursive)?;
        }
    }
    state.replace_watcher(watcher);
    Ok(())
}

pub async fn start_full_index(state: AppState) -> Result<()> {
    if !state.set_indexing(true) {
        return Ok(());
    }
    state.set_paused(false);
    let worker_state = state.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || run_full_index(&worker_state)).await?;
    state.set_current_file(None);
    state.set_indexing(false);
    state.emit_snapshot();
    result
}

fn run_full_index(state: &AppState) -> Result<()> {
    let settings = state.settings();
    emit_progress(
        state,
        IndexProgress {
            processed: 0,
            total: 0,
            current_file: None,
            status: IndexStatus::Scanning,
            message: "Finding supported files…".into(),
        },
    );

    let files = discover_files(&settings)?;
    let total = files.len() as u64;
    let all_paths = files
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    for (position, path) in files.iter().enumerate() {
        if state.is_paused() || state.is_shutting_down() {
            break;
        }
        let display_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
            .to_string();
        state.set_current_file(Some(display_name.clone()));
        emit_progress(
            state,
            IndexProgress {
                processed: position as u64,
                total,
                current_file: Some(display_name),
                status: IndexStatus::Indexing,
                message: "Reading and indexing locally…".into(),
            },
        );
        if let Err(error) = index_file(state, path) {
            tracing::warn!(path = %path.display(), %error, "could not index file");
            let metadata = std::fs::metadata(path).ok();
            let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("file");
            let extension = extension(path);
            let modified = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .map(system_time_string)
                .unwrap_or_default();
            let path_string = path.to_string_lossy().to_string();
            if let Ok(stale_ids) = state.storage().chunk_ids_for_path(&path_string) {
                let _ = delete_points(state, &stale_ids);
            }
            let _ = state.storage().mark_document_failed(
                &path_string,
                name,
                &extension,
                mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .essence_str(),
                &modified,
                metadata.map(|m| m.len()).unwrap_or_default(),
                &error.to_string(),
            );
        }
    }

    if !state.is_paused() {
        if let Ok(stale_ids) = state.storage().remove_missing_documents(&all_paths) {
            let _ = delete_points(state, &stale_ids);
        }
        let _ = state.with_shard(|shard| shard.optimize().map(|_| ()).map_err(Into::into));
    }
    emit_progress(
        state,
        IndexProgress {
            processed: total,
            total,
            current_file: None,
            status: if state.is_paused() {
                IndexStatus::Paused
            } else {
                IndexStatus::Idle
            },
            message: if state.is_paused() {
                "Indexing paused".into()
            } else {
                "Your library is up to date".into()
            },
        },
    );
    Ok(())
}

fn discover_files(settings: &crate::models::AppSettings) -> Result<Vec<PathBuf>> {
    let extensions = settings
        .include_extensions
        .iter()
        .map(|extension| extension.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let max_bytes = settings.max_file_size_mb * 1024 * 1024;
    let excluded = settings.excluded_paths.clone();
    let mut files = Vec::new();

    for root in &settings.library_paths {
        let root_path = Path::new(root);
        if !root_path.exists() {
            continue;
        }
        let mut builder = WalkBuilder::new(root_path);
        builder
            .hidden(true)
            .follow_links(false)
            .standard_filters(true);
        let excluded_for_filter = excluded.clone();
        builder.filter_entry(move |entry| {
            !entry.path().components().any(|component| {
                let value = component.as_os_str().to_string_lossy();
                excluded_for_filter
                    .iter()
                    .any(|excluded| excluded == &value)
            })
        });
        for entry in builder.build().flatten() {
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let path = entry.into_path();
            if !extensions.contains(&extension(&path)) {
                continue;
            }
            if std::fs::metadata(&path).is_ok_and(|meta| meta.len() <= max_bytes) {
                files.push(path);
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn index_file(state: &AppState, path: &Path) -> Result<()> {
    let metadata = std::fs::metadata(path)?;
    let modified_at = system_time_string(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
    let path_string = path.to_string_lossy().to_string();
    if state
        .storage()
        .document_is_current(&path_string, &modified_at, metadata.len())?
    {
        return Ok(());
    }

    let sections = extract_sections(path)?;
    let settings = state.settings();
    let chunks = chunk_sections(sections, settings.chunk_size, settings.chunk_overlap);
    anyhow::ensure!(!chunks.is_empty(), "no readable text was found");
    let content_hash = hash_chunks(&chunks);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = extension(path);
    let mime_type = mime_guess::from_path(path).first_or_octet_stream();
    let old_chunk_ids = state.storage().chunk_ids_for_path(&path_string)?;
    let (document_id, chunk_ids) = state.storage().replace_document(
        &path_string,
        name,
        &extension,
        mime_type.essence_str(),
        &modified_at,
        metadata.len(),
        &content_hash,
        &chunks,
    )?;
    delete_points(state, &old_chunk_ids)?;

    for (chunk_batch, id_batch) in chunks.chunks(32).zip(chunk_ids.chunks(32)) {
        let texts = chunk_batch
            .iter()
            .map(|chunk| chunk.content.clone())
            .collect::<Vec<_>>();
        let embeddings = state.embedder().embed_passages(&texts)?;
        let points = chunk_batch
            .iter()
            .zip(id_batch.iter())
            .zip(embeddings)
            .map(|((chunk, chunk_id), embedding)| {
                PointStruct::new(
                    *chunk_id,
                    Vectors::new_named([(VECTOR_NAME, embedding)]),
                    json!({
                        "chunk_id": chunk_id,
                        "document_id": document_id,
                        "path": path_string,
                        "page": chunk.page,
                    }),
                )
                .into()
            })
            .collect::<Vec<PointStructPersisted>>();
        state.with_shard(|shard| {
            shard
                .update(UpdateOperation::PointOperation(
                    PointOperations::UpsertPoints(PointInsertOperations::PointsList(points)),
                ))
                .map(|_| ())
                .map_err(Into::into)
        })?;
    }
    Ok(())
}

fn delete_points(state: &AppState, ids: &[u64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    state.with_shard(|shard| {
        shard
            .update(UpdateOperation::PointOperation(
                PointOperations::DeletePoints {
                    ids: ids.iter().copied().map(Into::into).collect(),
                },
            ))
            .map(|_| ())
            .map_err(Into::into)
    })
}

#[derive(Debug)]
struct ParsedSection {
    text: String,
    page: Option<u32>,
    heading: Option<String>,
}

fn extract_sections(path: &Path) -> Result<Vec<ParsedSection>> {
    match extension(path).as_str() {
        "pdf" => extract_pdf(path),
        "docx" => extract_docx(path),
        "html" | "htm" | "xml" => extract_markup(path),
        _ => extract_plain_text(path),
    }
}

fn extract_pdf(path: &Path) -> Result<Vec<ParsedSection>> {
    let text = pdf_extract::extract_text(path)
        .with_context(|| format!("could not read PDF {}", path.display()))?;
    let pages = text
        .split('\u{000c}')
        .enumerate()
        .filter_map(|(index, text)| {
            let text = clean_text(text);
            (!text.is_empty()).then_some(ParsedSection {
                text,
                page: Some(index as u32 + 1),
                heading: None,
            })
        })
        .collect::<Vec<_>>();
    if pages.is_empty() {
        Ok(vec![ParsedSection {
            text: clean_text(&text),
            page: Some(1),
            heading: None,
        }])
    } else {
        Ok(pages)
    }
}

fn extract_docx(path: &Path) -> Result<Vec<ParsedSection>> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut document = String::new();
    archive
        .by_name("word/document.xml")?
        .read_to_string(&mut document)?;
    let mut reader = Reader::from_str(&document);
    reader.config_mut().trim_text(true);
    let mut paragraphs = Vec::new();
    let mut current = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(text)) => {
                let decoded = text.decode()?;
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(&decoded);
            }
            Ok(Event::End(end)) if end.name().as_ref() == b"w:p" => {
                let paragraph = clean_text(&current);
                if !paragraph.is_empty() {
                    paragraphs.push(paragraph);
                }
                current.clear();
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(error.into()),
            _ => {}
        }
    }
    Ok(vec![ParsedSection {
        text: paragraphs.join("\n\n"),
        page: None,
        heading: None,
    }])
}

fn extract_markup(path: &Path) -> Result<Vec<ParsedSection>> {
    let raw = std::fs::read_to_string(path)?;
    let mut reader = Reader::from_str(&raw);
    reader.config_mut().trim_text(true);
    let mut output = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(text)) => {
                let decoded = text.decode()?;
                if !output.is_empty() {
                    output.push(' ');
                }
                output.push_str(&decoded);
            }
            Ok(Event::Eof) => break,
            Err(_) => return extract_plain_text(path),
            _ => {}
        }
    }
    Ok(vec![ParsedSection {
        text: clean_text(&output),
        page: None,
        heading: None,
    }])
}

fn extract_plain_text(path: &Path) -> Result<Vec<ParsedSection>> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(vec![ParsedSection {
        text: clean_text(&text),
        page: None,
        heading: None,
    }])
}

fn chunk_sections(
    sections: Vec<ParsedSection>,
    target_size: usize,
    overlap: usize,
) -> Vec<ChunkInput> {
    let mut chunks = Vec::new();
    for section in sections {
        let characters = section.text.chars().collect::<Vec<_>>();
        if characters.is_empty() {
            continue;
        }
        let mut start = 0;
        while start < characters.len() {
            let upper = (start + target_size).min(characters.len());
            let mut end = upper;
            if upper < characters.len() {
                for candidate in (start + target_size / 2..upper).rev() {
                    if matches!(
                        characters[candidate],
                        '\n' | '.' | '!' | '?' | '。' | '！' | '？'
                    ) {
                        end = candidate + 1;
                        break;
                    }
                }
            }
            let content = clean_text(&characters[start..end].iter().collect::<String>());
            if content.chars().count() >= 20 {
                chunks.push(ChunkInput {
                    content,
                    page: section.page,
                    heading: section.heading.clone(),
                });
            }
            if end >= characters.len() {
                break;
            }
            start = end.saturating_sub(overlap).max(start + 1);
        }
    }
    chunks
}

fn clean_text(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn hash_chunks(chunks: &[ChunkInput]) -> String {
    let mut hasher = Sha256::new();
    for chunk in chunks {
        hasher.update(chunk.content.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn system_time_string(value: SystemTime) -> String {
    DateTime::<Utc>::from(value).to_rfc3339()
}

fn emit_progress(state: &AppState, progress: IndexProgress) {
    let _ = state.app_handle().emit("index-progress", progress);
    state.emit_snapshot();
}
