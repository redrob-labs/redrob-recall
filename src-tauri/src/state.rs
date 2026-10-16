use crate::{
    embedding::LocalEmbedder,
    engine::{Engine, EngineError},
    indexer, local_api,
    models::{AppSettings, AppSnapshot, ConnectionStatus, IndexStatus},
    shared_account::{self, SharedAccount},
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
    /// The shared Redrob Code daemon, which holds the Redrob console key. This app has none.
    pub engine: Engine,
    /// What the daemon last said about its sign-in. Snapshots are emitted far too often to ask
    /// the daemon each time, so `refresh_connection` asks and everything else reads this.
    pub connection: RwLock<ConnectionStatus>,
    /// Redrob Launcher's shared sign-in as last read from its `account.json`; `None` when the
    /// computer is not signed in that way. Re-read at startup, on every connection refresh and
    /// whenever the watcher sees the file change.
    pub shared_account: RwLock<Option<SharedAccount>>,
    /// Keeps the shared-account directory watch alive; dropped on shutdown.
    pub shared_account_watcher: Mutex<Option<notify::RecommendedWatcher>>,
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

impl AppState {
    pub fn initialize(app_handle: AppHandle) -> Result<Self> {
        let base_dirs = BaseDirs::new().context("home directory is unavailable")?;
        let data_dir = base_dirs.home_dir().join(".redrob").join("recall");
        Self::open(Some(app_handle), data_dir, Engine::new())
    }

    /// The real state, rooted somewhere other than the user's library and with nothing to emit to.
    /// Test-only: the indexer integration tests drive the same code the app does, without touching
    /// `~/.redrob/recall` or the OS keyring.
    #[cfg(test)]
    pub(crate) fn open_headless(data_dir: PathBuf) -> Result<Self> {
        Self::open(None, data_dir, Engine::new())
    }

    /// `open_headless` against a given engine, for the sign-in tests: a fake daemon, never the
    /// real one on this machine.
    #[cfg(test)]
    pub(crate) fn open_headless_with_engine(data_dir: PathBuf, engine: Engine) -> Result<Self> {
        Self::open(None, data_dir, engine)
    }

    fn open(app_handle: Option<AppHandle>, data_dir: PathBuf, engine: Engine) -> Result<Self> {
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
            engine,
            connection: RwLock::new(ConnectionStatus::unknown()),
            shared_account: RwLock::new(shared_account::read()),
            shared_account_watcher: Mutex::new(None),
            indexing: AtomicBool::new(false),
            rescan_requested: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            current_file: RwLock::new(None),
            watcher: Mutex::new(None),
        })))
    }

    pub fn start_background_services(&self) {
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            state.migrate_legacy_key().await;
            state.refresh_connection().await;
        });
        self.watch_shared_account();
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
        self.0.shared_account_watcher.lock().take();
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

    /// What the daemon last said about its sign-in. `refresh_connection` is what asks it.
    pub fn connection_status(&self) -> ConnectionStatus {
        self.0.connection.read().clone()
    }

    pub fn engine(&self) -> &Engine {
        &self.0.engine
    }

    /// Redrob Launcher's shared sign-in, as last read. `refresh_connection` re-reads it.
    pub fn shared_account(&self) -> Option<SharedAccount> {
        self.0.shared_account.read().clone()
    }

    /// Re-reads the launcher's `account.json` whenever it changes, so a sign-in or sign-out there
    /// is reflected here while the app is open. Without a watchable directory the file is still
    /// re-read on every connection refresh (startup, focus, each question).
    fn watch_shared_account(&self) {
        let state = self.clone();
        match shared_account::watch(move || {
            let state = state.clone();
            tauri::async_runtime::spawn(async move {
                state.refresh_connection().await;
            });
        }) {
            Ok(watcher) => *self.0.shared_account_watcher.lock() = watcher,
            Err(error) => tracing::warn!(%error, "the shared account file could not be watched"),
        }
    }

    /// Reads the sign-in this computer has and remembers it. Redrob Launcher's shared sign-in
    /// counts first: when its file holds a key, this app is signed in with it and asks nothing of
    /// the daemon. Otherwise the daemon is asked whether it is signed in -- found or started on the
    /// way, so a missing engine is reported here first.
    pub async fn refresh_connection(&self) -> ConnectionStatus {
        let shared = shared_account::read();
        *self.0.shared_account.write() = shared.clone();
        if let Some(account) = shared {
            let status = ConnectionStatus {
                connected: true,
                key_label: Some(account.label().unwrap_or_else(|| "Redrob Launcher".into())),
                via_launcher: true,
                endpoint: account.api_base_url.clone(),
                error: None,
            };
            *self.0.connection.write() = status.clone();
            self.emit_snapshot();
            return status;
        }
        let status = match self.0.engine.account_status().await {
            Ok(account) => ConnectionStatus {
                connected: account.connected,
                key_label: account.connected.then(|| {
                    if account.is_from_launcher() {
                        "Redrob Launcher".to_string()
                    } else {
                        "Redrob Code".to_string()
                    }
                }),
                via_launcher: account.is_from_launcher(),
                endpoint: self
                    .0
                    .engine
                    .connect()
                    .await
                    .map(|record| record.url)
                    .unwrap_or_default(),
                error: None,
            },
            Err(error) => {
                tracing::warn!(%error, "the Redrob Code engine is unavailable");
                ConnectionStatus {
                    connected: false,
                    key_label: None,
                    via_launcher: false,
                    endpoint: String::new(),
                    error: Some(error.to_string()),
                }
            }
        };
        *self.0.connection.write() = status.clone();
        self.emit_snapshot();
        status
    }

    /// Hands a console key to the daemon, which keeps it. Nothing of this app's stores it.
    pub async fn sign_in(&self, api_key: String) -> Result<ConnectionStatus, String> {
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            return Err("API key cannot be empty".into());
        }
        if trimmed.len() < 16 {
            return Err("that API key is too short".into());
        }
        self.0
            .engine
            .sign_in(trimmed)
            .await
            .map_err(|error| error.to_string())?;
        Ok(self.refresh_connection().await)
    }

    /// Removes the key from the daemon. Redrob Launcher's shared sign-in is the engine's to keep
    /// reading; signing out of that happens in the launcher, and the UI offers no button for it.
    pub async fn sign_out(&self) -> Result<ConnectionStatus, String> {
        self.0
            .engine
            .sign_out()
            .await
            .map_err(|error| error.to_string())?;
        Ok(self.refresh_connection().await)
    }

    /// An earlier version of this app kept the console key in the OS credential store. The engine
    /// holds it now: hand a key found there over once, then delete it. A key that cannot be handed
    /// over yet (no engine) stays for the next run.
    async fn migrate_legacy_key(&self) {
        let Some(key) = legacy_key() else { return };
        match self.0.engine.sign_in(&key).await {
            Ok(()) => {
                forget_legacy_key();
                tracing::info!("the stored Redrob key was handed to the Redrob Code engine");
            }
            Err(EngineError::Unavailable(_)) => {}
            Err(error) => tracing::warn!(%error, "the stored Redrob key could not be handed over"),
        }
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

const LEGACY_KEYRING_SERVICE: &str = "ai.redrob.recall";
const LEGACY_KEYRING_USER: &str = "redrob-api-key";

/// The key an earlier version stored in the OS credential store, if any. Migration only.
fn legacy_key() -> Option<String> {
    let entry = keyring::Entry::new(LEGACY_KEYRING_SERVICE, LEGACY_KEYRING_USER).ok()?;
    entry
        .get_password()
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

fn forget_legacy_key() {
    if let Ok(entry) = keyring::Entry::new(LEGACY_KEYRING_SERVICE, LEGACY_KEYRING_USER) {
        let _ = entry.delete_credential();
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::engine::tests::FakeDaemon;
    use crate::engine::Engine;
    use redrob_engine_client::Unavailable;
    use std::sync::{Arc, Mutex};

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("redrob-recall-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn the_connection_reads_the_daemon_sign_in_and_hands_keys_to_it() {
        let dir = scratch("engine-connection");
        // This test is about the daemon path: point the shared-account lookup at an empty
        // directory so a developer machine signed in through Redrob Launcher does not take over.
        std::env::set_var("REDROB_SHARED_DIR", dir.join("shared"));
        // The daemon's provider list, as the test moves it along: signed out, then signed in
        // with the launcher's key, then with a key an app handed over.
        let providers = Arc::new(Mutex::new(r#"{"all":[]}"#.to_string()));
        let answers = providers.clone();
        let daemon = FakeDaemon::start("s", move |seen, _| {
            match (seen.method.as_str(), seen.path.as_str()) {
                ("GET", "/provider") => (200, answers.lock().unwrap().clone()),
                ("PUT", "/auth/redrob") => {
                    *answers.lock().unwrap() =
                        r#"{"all":[{"id":"redrob","source":"api","key":"set"}]}"#.into();
                    (200, "true".into())
                }
                ("DELETE", "/auth/redrob") => {
                    *answers.lock().unwrap() = r#"{"all":[]}"#.into();
                    (200, "true".into())
                }
                _ => (404, "{}".into()),
            }
        });
        let state =
            AppState::open_headless_with_engine(dir.join("data"), daemon.engine("s")).unwrap();

        // Before anyone asked, nothing is claimed.
        let status = state.connection_status();
        assert!(!status.connected && status.error.is_none());

        // Signed out: not connected, and the engine's address is known.
        let status = state.refresh_connection().await;
        assert!(!status.connected && !status.via_launcher && status.key_label.is_none());
        assert_eq!(status.endpoint, daemon.url);
        assert_eq!(status.error, None);

        // Signed in through Redrob Launcher: the engine reads that itself and says so.
        *providers.lock().unwrap() = r#"{"all":[{"id":"redrob","source":"launcher"}]}"#.into();
        let status = state.refresh_connection().await;
        assert!(status.connected && status.via_launcher);
        assert_eq!(status.key_label.as_deref(), Some("Redrob Launcher"));
        assert!(state.connection_status().connected, "remembered");

        // A pasted key goes to the engine, is too short to send when too short, and signing out
        // takes it back from the engine.
        assert!(state.sign_in("short".into()).await.is_err());
        let status = state.sign_in(" rk_0123456789abcdef ".into()).await.unwrap();
        assert!(status.connected && !status.via_launcher);
        assert_eq!(status.key_label.as_deref(), Some("Redrob Code"));
        let put = daemon
            .seen
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.method == "PUT")
            .cloned()
            .unwrap();
        assert!(
            put.body.contains(r#""key":"rk_0123456789abcdef""#),
            "trimmed"
        );
        let status = state.sign_out().await.unwrap();
        assert!(!status.connected);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_missing_engine_is_reported_on_the_connection_and_blocks_sign_in() {
        let dir = scratch("engine-missing-connection");
        std::env::set_var("REDROB_SHARED_DIR", dir.join("shared"));
        let engine = Engine::with_resolver(Arc::new(|| Err(Unavailable::NotInstalled)));
        let state = AppState::open_headless_with_engine(dir.join("data"), engine).unwrap();
        let status = state.refresh_connection().await;
        assert!(!status.connected);
        assert_eq!(
            status.error.as_deref(),
            Some("Redrob Code is not installed. Install it with Redrob Launcher. https://github.com/redrob-labs/redrob-launcher/releases/latest")
        );
        assert_eq!(
            state
                .sign_in("rk_0123456789abcdef".into())
                .await
                .unwrap_err(),
            status.error.unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
