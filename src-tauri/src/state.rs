use crate::{
    embedding::LocalEmbedder,
    indexer, local_api,
    models::{AppSettings, AppSnapshot, ConnectionStatus, IndexStatus},
    shared_account,
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
    /// `None` only in tests, which run the real indexer without a window to emit to.
    pub app_handle: Option<AppHandle>,
    pub data_dir: PathBuf,
    pub storage: Storage,
    pub shard: Mutex<Option<EdgeShard>>,
    pub embedder: LocalEmbedder,
    pub settings: RwLock<AppSettings>,
    /// This app's own key, from the OS keyring. Wins over the shared one.
    pub api_key: RwLock<Option<String>>,
    /// Redrob Launcher's shared account file, the fallback when this app has no key of its own.
    /// Read fresh on every use: the person may sign in or out of the launcher while this runs.
    pub shared_account_path: Option<PathBuf>,
    pub indexing: AtomicBool,
    /// Set when an index was asked for while one was already running, so the running pass runs
    /// once more when it finishes instead of the request being dropped.
    pub rescan_requested: AtomicBool,
    pub paused: AtomicBool,
    pub shutting_down: AtomicBool,
    pub current_file: RwLock<Option<String>>,
    pub watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

/// Which key `AppState` resolved to.
enum KeyInUse {
    Own(String),
    Shared(shared_account::SharedAccount),
}

impl AppState {
    pub fn initialize(app_handle: AppHandle) -> Result<Self> {
        let base_dirs = BaseDirs::new().context("home directory is unavailable")?;
        let data_dir = base_dirs.home_dir().join(".redrob").join("recall");
        Self::open(
            Some(app_handle),
            data_dir,
            load_api_key().ok(),
            shared_account::shared_account_path(),
        )
    }

    /// The real state, rooted somewhere other than the user's library and with nothing to emit to.
    /// Test-only: the indexer integration tests drive the same code the app does, without touching
    /// `~/.redrob/recall` or the OS keyring.
    #[cfg(test)]
    pub(crate) fn open_headless(data_dir: PathBuf) -> Result<Self> {
        Self::open(None, data_dir, None, None)
    }

    /// `open_headless` with an own key and a shared account file, for the sign-in tests. The own
    /// key is given directly so the tests never touch the OS keyring.
    #[cfg(test)]
    pub(crate) fn open_headless_with_keys(
        data_dir: PathBuf,
        own: Option<String>,
        shared: PathBuf,
    ) -> Result<Self> {
        Self::open(None, data_dir, own, Some(shared))
    }

    fn open(
        app_handle: Option<AppHandle>,
        data_dir: PathBuf,
        api_key: Option<String>,
        shared_account_path: Option<PathBuf>,
    ) -> Result<Self> {
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

        Ok(Self(Arc::new(Inner {
            app_handle,
            data_dir,
            storage,
            shard: Mutex::new(Some(shard)),
            embedder,
            settings: RwLock::new(settings),
            api_key: RwLock::new(api_key),
            shared_account_path,
            indexing: AtomicBool::new(false),
            rescan_requested: AtomicBool::new(false),
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
        let settings = self.settings();
        let folders = self.0.storage.folder_stats(&settings.library_paths)?;
        Ok(AppSnapshot {
            folders,
            settings,
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
        let key = self.key_in_use();
        ConnectionStatus {
            connected: key.is_some(),
            key_label: key.as_ref().map(|key| match key {
                KeyInUse::Own(_) => "This device".to_string(),
                KeyInUse::Shared(account) => account.api_key_name.clone(),
            }),
            via_launcher: matches!(key, Some(KeyInUse::Shared(_))),
            endpoint: settings.redrob_base_url,
            error: None,
        }
    }

    /// The key this app uses now: its own when it has one, else the computer's shared one from
    /// Redrob Launcher, read fresh each time. `None` when neither is signed in.
    fn key_in_use(&self) -> Option<KeyInUse> {
        if let Some(own) = self.0.api_key.read().clone() {
            return Some(KeyInUse::Own(own));
        }
        self.0
            .shared_account_path
            .as_deref()
            .and_then(shared_account::load)
            .map(KeyInUse::Shared)
    }

    pub fn set_api_key(&self, api_key: String) -> Result<()> {
        let trimmed = api_key.trim();
        anyhow::ensure!(!trimmed.is_empty(), "API key cannot be empty");
        anyhow::ensure!(trimmed.len() >= 16, "that API key is too short");
        let entry = keyring::Entry::new("ai.redrob.recall", "redrob-api-key")
            .context("the operating system credential store is unavailable")?;
        entry
            .set_password(trimmed)
            .context("the API key could not be saved in the operating system credential store")?;
        *self.0.api_key.write() = Some(trimmed.to_string());
        self.emit_snapshot();
        Ok(())
    }

    /// Forgets this app's own key only. The launcher's shared file is not this app's to delete;
    /// signing out of it happens in Redrob Launcher.
    pub fn disconnect(&self) {
        *self.0.api_key.write() = None;
        if let Ok(entry) = keyring::Entry::new("ai.redrob.recall", "redrob-api-key") {
            let _ = entry.delete_credential();
        }
        self.emit_snapshot();
    }

    /// The bearer token to spend: this app's own key, else Redrob Launcher's shared one.
    pub fn api_key(&self) -> Option<String> {
        self.key_in_use().map(|key| match key {
            KeyInUse::Own(key) => key,
            KeyInUse::Shared(account) => account.api_key,
        })
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

    pub fn request_rescan(&self) {
        self.0.rescan_requested.store(true, Ordering::SeqCst);
    }

    /// Consume a pending rescan request.
    pub fn take_rescan_request(&self) -> bool {
        self.0.rescan_requested.swap(false, Ordering::SeqCst)
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

    /// Emit to the window, if there is one. Emission is best-effort everywhere it is used.
    pub fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(app_handle) = &self.0.app_handle {
            let _ = app_handle.emit(event, payload);
        }
    }

    pub fn emit_snapshot(&self) {
        if let Ok(snapshot) = self.snapshot() {
            self.emit("snapshot-changed", snapshot);
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
    let entry = keyring::Entry::new("ai.redrob.recall", "redrob-api-key")?;
    entry
        .get_password()
        .context("no Redrob API key in the OS keyring")
}

#[cfg(test)]
mod tests {
    use super::AppState;

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("redrob-recall-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn shared_file(path: &std::path::Path, key: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            format!(
                r#"{{"apiKey":"{key}","apiKeyName":"Redrob Launcher","accountName":"Acme","apiBaseUrl":"https://c/v1"}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn the_launcher_key_is_the_fallback_and_the_apps_own_key_wins() {
        let dir = scratch("shared-account");
        let shared = dir.join("shared").join("account.json");

        // No key anywhere: not connected, and asking has no bearer token.
        let state =
            AppState::open_headless_with_keys(dir.join("data-a"), None, shared.clone()).unwrap();
        let status = state.connection_status();
        assert!(!status.connected && !status.via_launcher && status.key_label.is_none());
        assert_eq!(state.api_key(), None);

        // Signed in through Redrob Launcher while the app is open: picked up on the next read,
        // marked as the launcher's, and the shared key is what gets spent.
        shared_file(&shared, "rk_shared_0123456789");
        let status = state.connection_status();
        assert!(status.connected && status.via_launcher);
        assert_eq!(status.key_label.as_deref(), Some("Redrob Launcher"));
        assert_eq!(state.api_key().as_deref(), Some("rk_shared_0123456789"));

        // A broken or emptied file reads as signed out, without an error.
        std::fs::write(&shared, "{").unwrap();
        assert!(!state.connection_status().connected);
        std::fs::write(&shared, r#"{"apiKey":""}"#).unwrap();
        assert!(!state.connection_status().connected);

        // Signed out in the launcher: nothing left.
        shared_file(&shared, "rk_shared_0123456789");
        assert!(state.connection_status().connected);
        std::fs::remove_file(&shared).unwrap();
        assert!(!state.connection_status().connected);
        assert_eq!(state.api_key(), None);

        // This app's own key wins over the launcher's, and is labelled as this device's.
        shared_file(&shared, "rk_shared_0123456789");
        let state = AppState::open_headless_with_keys(
            dir.join("data-b"),
            Some("rk_own_0123456789".into()),
            shared.clone(),
        )
        .unwrap();
        let status = state.connection_status();
        assert!(status.connected && !status.via_launcher);
        assert_eq!(status.key_label.as_deref(), Some("This device"));
        assert_eq!(state.api_key().as_deref(), Some("rk_own_0123456789"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
