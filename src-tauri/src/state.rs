use crate::{
    embedding::LocalEmbedder,
    indexer, local_api,
    models::{AppSettings, AppSnapshot, ConnectionStatus, IndexStatus},
    storage::Storage,
};
use anyhow::{Context, Result};
use directories::BaseDirs;
use parking_lot::{Mutex, RwLock};
use qdrant_edge::{Distance, EdgeConfigBuilder, EdgeShard, EdgeVectorParamsBuilder};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::{AppHandle, Emitter};

use crate::models::{EMBEDDING_DIMENSION, VECTOR_NAME};

struct Inner {
    pub app_handle: AppHandle,
    pub data_dir: PathBuf,
    pub storage: Storage,
    pub shard: Mutex<Option<EdgeShard>>,
    pub embedder: LocalEmbedder,
    pub settings: RwLock<AppSettings>,
    pub api_key: RwLock<Option<String>>,
    pub indexing: AtomicBool,
    pub paused: AtomicBool,
    pub shutting_down: AtomicBool,
    pub current_file: RwLock<Option<String>>,
    pub watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl AppState {
    pub fn initialize(app_handle: AppHandle) -> Result<Self> {
        let base_dirs = BaseDirs::new().context("home directory is unavailable")?;
        let data_dir = base_dirs.home_dir().join(".redrob").join("vectordb");
        std::fs::create_dir_all(&data_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let metadata_path = data_dir.join("metadata.db");
        let storage = match Storage::open(&metadata_path) {
            Ok(storage) => storage,
            Err(error) => {
                if !metadata_path.is_file() || !Storage::is_confirmed_corrupt(&metadata_path) {
                    return Err(error);
                }
                let quarantine = quarantine_metadata_database(&metadata_path)?;
                tracing::error!(%error, destination = %quarantine.display(), "metadata database was quarantined and will be rebuilt");
                Storage::open(&metadata_path)?
            }
        };
        let settings = storage.load_settings()?;
        let (shard, recovered_shard) = open_shard(&data_dir.join("qdrant-edge"))?;
        if recovered_shard {
            storage.mark_all_vectors_pending()?;
        }
        let embedder = LocalEmbedder::new(&data_dir.join("models"));
        let api_key = load_api_key().ok();

        Ok(Self(Arc::new(Inner {
            app_handle,
            data_dir,
            storage,
            shard: Mutex::new(Some(shard)),
            embedder,
            settings: RwLock::new(settings),
            api_key: RwLock::new(api_key),
            indexing: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            current_file: RwLock::new(None),
            watcher: Mutex::new(None),
        })))
    }

    pub fn start_background_services(&self) {
        if let Err(error) = indexer::configure_watcher(self) {
            tracing::warn!(%error, "file watcher could not start");
        }
        if !self.settings().library_paths.is_empty() {
            let state = self.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = indexer::start_full_index(state).await {
                    tracing::warn!(%error, "startup library scan failed");
                }
            });
        }
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = local_api::serve(state).await {
                tracing::warn!(%error, "local API stopped");
            }
        });
    }

    pub fn shutdown(&self) {
        self.0.shutting_down.store(true, Ordering::SeqCst);
        self.0.watcher.lock().take();
        self.0.shard.lock().take();
    }

    pub fn snapshot(&self) -> Result<AppSnapshot> {
        let status = if self.0.paused.load(Ordering::SeqCst) {
            IndexStatus::Paused
        } else if self.0.indexing.load(Ordering::SeqCst) {
            IndexStatus::Indexing
        } else {
            IndexStatus::Idle
        };
        Ok(AppSnapshot {
            settings: self.settings(),
            stats: self
                .0
                .storage
                .stats(status, self.0.current_file.read().clone())?,
            connection: self.connection_status(),
            data_directory: self.0.data_dir.to_string_lossy().to_string(),
            app_version: env!("CARGO_PKG_VERSION").into(),
        })
    }

    pub fn settings(&self) -> AppSettings {
        self.0.settings.read().clone()
    }

    pub(crate) fn persist_settings(&self, settings: AppSettings) -> Result<()> {
        self.0.storage.save_settings(&settings)?;
        *self.0.settings.write() = settings;
        Ok(())
    }

    pub fn update_settings(&self, settings: AppSettings) -> Result<()> {
        self.persist_settings(settings)?;
        indexer::configure_watcher(self)?;
        self.emit_snapshot();
        Ok(())
    }

    pub fn connection_status(&self) -> ConnectionStatus {
        let settings = self.settings();
        ConnectionStatus {
            connected: self.0.api_key.read().is_some(),
            key_label: self
                .0
                .api_key
                .read()
                .as_ref()
                .map(|_| "This device".to_string()),
            endpoint: settings.redrob_base_url,
            error: None,
        }
    }

    pub fn set_api_key(&self, api_key: String) -> Result<()> {
        let trimmed = api_key.trim();
        anyhow::ensure!(!trimmed.is_empty(), "API key cannot be empty");
        anyhow::ensure!(trimmed.len() >= 16, "that API key is too short");
        let entry = keyring::Entry::new("ai.redrob.vectordb", "redrob-api-key")
            .context("the operating system credential store is unavailable")?;
        entry
            .set_password(trimmed)
            .context("the API key could not be saved in the operating system credential store")?;
        *self.0.api_key.write() = Some(trimmed.to_string());
        self.emit_snapshot();
        Ok(())
    }

    pub fn disconnect(&self) {
        *self.0.api_key.write() = None;
        if let Ok(entry) = keyring::Entry::new("ai.redrob.vectordb", "redrob-api-key") {
            let _ = entry.delete_credential();
        }
        self.emit_snapshot();
    }

    pub fn api_key(&self) -> Option<String> {
        self.0.api_key.read().clone()
    }

    pub fn data_dir(&self) -> &Path {
        &self.0.data_dir
    }

    pub fn storage(&self) -> &Storage {
        &self.0.storage
    }

    pub fn embedder(&self) -> &LocalEmbedder {
        &self.0.embedder
    }

    pub fn with_shard<T>(&self, operation: impl FnOnce(&EdgeShard) -> Result<T>) -> Result<T> {
        let guard = self.0.shard.lock();
        let shard = guard.as_ref().context("local vector index is closed")?;
        operation(shard)
    }

    pub fn set_indexing(&self, value: bool) -> bool {
        if value {
            self.0
                .indexing
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        } else {
            self.0.indexing.store(false, Ordering::SeqCst);
            true
        }
    }

    pub fn is_indexing(&self) -> bool {
        self.0.indexing.load(Ordering::SeqCst)
    }

    pub fn set_paused(&self, paused: bool) {
        self.0.paused.store(paused, Ordering::SeqCst);
        self.emit_snapshot();
    }

    pub fn is_paused(&self) -> bool {
        self.0.paused.load(Ordering::SeqCst)
    }

    pub fn is_shutting_down(&self) -> bool {
        self.0.shutting_down.load(Ordering::SeqCst)
    }

    pub fn set_current_file(&self, file: Option<String>) {
        *self.0.current_file.write() = file;
    }

    pub fn replace_watcher(&self, watcher: notify::RecommendedWatcher) {
        *self.0.watcher.lock() = Some(watcher);
    }

    pub fn app_handle(&self) -> &AppHandle {
        &self.0.app_handle
    }

    pub fn emit_snapshot(&self) {
        if let Ok(snapshot) = self.snapshot() {
            let _ = self.0.app_handle.emit("snapshot-changed", snapshot);
        }
    }

    pub fn reset_shard(&self) -> Result<()> {
        self.0.shard.lock().take();
        let path = self.0.data_dir.join("qdrant-edge");
        if path.exists() {
            std::fs::remove_dir_all(&path)?;
        }
        let (shard, _) = open_shard(&path)?;
        *self.0.shard.lock() = Some(shard);
        Ok(())
    }
}

fn quarantine_metadata_database(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .context("metadata database has no parent directory")?;
    let destination = parent.join(format!(
        "metadata-corrupt-{}-{}.db",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::rename(path, &destination)
        .context("failed to quarantine the damaged metadata database")?;
    for suffix in ["-wal", "-shm"] {
        let sidecar = path_with_suffix(path, suffix);
        if sidecar.try_exists()? {
            let sidecar_destination = path_with_suffix(&destination, suffix);
            std::fs::rename(&sidecar, &sidecar_destination).with_context(|| {
                format!(
                    "failed to quarantine metadata sidecar {}",
                    sidecar.display()
                )
            })?;
        }
    }
    Ok(destination)
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn open_shard(path: &Path) -> Result<(EdgeShard, bool)> {
    std::fs::create_dir_all(path)?;
    let has_data = std::fs::read_dir(path)?.next().transpose()?.is_some();
    if has_data {
        match EdgeShard::load(path, None) {
            Ok(shard) => return Ok((shard, false)),
            Err(error) => {
                let quarantine = path.with_file_name(format!(
                    "qdrant-edge-corrupt-{}",
                    chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
                ));
                tracing::error!(%error, destination = %quarantine.display(), "local vector index was quarantined and will be rebuilt");
                std::fs::rename(path, &quarantine)
                    .context("failed to quarantine the damaged local vector index")?;
                std::fs::create_dir_all(path)?;
            }
        }
    }
    let config = EdgeConfigBuilder::new()
        .on_disk_payload(true)
        .vector(
            VECTOR_NAME,
            EdgeVectorParamsBuilder::new(EMBEDDING_DIMENSION, Distance::Cosine)
                .on_disk(true)
                .build(),
        )
        .max_search_threads(4)
        .build();
    let shard =
        EdgeShard::new(path, config).context("failed to create the local Qdrant Edge index")?;
    Ok((shard, true))
}

fn load_api_key() -> Result<String> {
    let entry = keyring::Entry::new("ai.redrob.vectordb", "redrob-api-key")?;
    entry
        .get_password()
        .context("no Redrob API key in the OS keyring")
}
