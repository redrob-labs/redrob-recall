// SPDX-License-Identifier: Apache-2.0
//! The library: one data directory, its stores, its settings and the indexing state. Promoted from
//! the Tauri app's `state.rs` and the folder commands of `commands.rs`; the window is gone, so what
//! used to be emitted as events is read from [`Library::snapshot`] and [`Library::progress`], and a
//! change callback ([`Library::set_on_change`]) wakes whoever draws them.

use crate::{
    embedding::LocalEmbedder,
    indexer,
    models::{AppSettings, FolderStatus, IndexProgress, IndexStats, IndexStatus, LibrarySnapshot},
    storage::Storage,
};
use anyhow::{Context, Result};
use directories::BaseDirs;
use parking_lot::{Mutex, RwLock};
use qdrant_edge::{Distance, EdgeConfigBuilder, EdgeShard, EdgeVectorParamsBuilder};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::models::{EMBEDDING_DIMENSION, VECTOR_NAME};

/// Called (from any thread) whenever the snapshot may have changed: a pass started or finished, a
/// file was indexed, settings were saved. The desktop shell asks egui to repaint from it.
pub type OnChange = Arc<dyn Fn() + Send + Sync>;

struct Inner {
    data_dir: PathBuf,
    storage: Storage,
    shard: Mutex<Option<EdgeShard>>,
    embedder: LocalEmbedder,
    settings: RwLock<AppSettings>,
    indexing: AtomicBool,
    /// Set when an index was asked for while one was already running, so the running pass runs
    /// once more when it finishes instead of the request being dropped.
    rescan_requested: AtomicBool,
    paused: AtomicBool,
    shutting_down: AtomicBool,
    current_file: RwLock<Option<String>>,
    progress: RwLock<Option<IndexProgress>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    on_change: RwLock<Option<OnChange>>,
}

/// A handle to the one library of a process. Cheap to clone; every clone shares the stores.
#[derive(Clone)]
pub struct Library(Arc<Inner>);

/// `~/.redrob/recall`: where the metadata database, the vector index, the embedding model cache and
/// the local API token live. `REDROB_RECALL_DATA_DIR` overrides it (tests, portable setups).
pub fn default_data_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("REDROB_RECALL_DATA_DIR").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let base_dirs = BaseDirs::new().context("home directory is unavailable")?;
    Ok(base_dirs.home_dir().join(".redrob").join("recall"))
}

impl Library {
    /// Open (or create) the library at `data_dir`. A damaged metadata database or vector index is
    /// quarantined beside itself and rebuilt rather than refused.
    pub fn open(data_dir: PathBuf) -> Result<Self> {
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
            data_dir,
            storage,
            shard: Mutex::new(Some(shard)),
            embedder,
            settings: RwLock::new(settings),
            indexing: AtomicBool::new(false),
            rescan_requested: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            current_file: RwLock::new(None),
            progress: RwLock::new(None),
            watcher: Mutex::new(None),
            on_change: RwLock::new(None),
        })))
    }

    /// The real library, rooted somewhere other than the user's `~/.redrob/recall`. Test-only: the
    /// indexer integration tests drive the same code the app does.
    #[cfg(test)]
    pub(crate) fn open_headless(data_dir: PathBuf) -> Result<Self> {
        Self::open(data_dir)
    }

    /// Start the file watcher and, when folders are configured, a first pass over them.
    pub fn start_background_services(&self) {
        if let Err(error) = indexer::configure_watcher(self) {
            tracing::warn!(%error, "file watcher could not start");
        }
        if !self.settings().library_paths.is_empty() {
            indexer::request_index(self);
        }
    }

    /// Who to wake when the snapshot changes (one listener; the latest wins).
    pub fn set_on_change(&self, on_change: OnChange) {
        *self.0.on_change.write() = Some(on_change);
    }

    pub fn shutdown(&self) {
        self.0.shutting_down.store(true, Ordering::SeqCst);
        self.0.watcher.lock().take();
        self.0.shard.lock().take();
    }

    pub fn snapshot(&self) -> Result<LibrarySnapshot> {
        let status = self.status();
        let settings = self.settings();
        let folders = self.0.storage.folder_stats(&settings.library_paths)?;
        Ok(LibrarySnapshot {
            folders,
            settings,
            stats: self
                .0
                .storage
                .stats(status, self.0.current_file.read().clone())?,
            data_directory: self.0.data_dir.to_string_lossy().to_string(),
            version: env!("CARGO_PKG_VERSION").into(),
        })
    }

    /// The stats alone, for a status line that does not need the folder list.
    pub fn stats(&self) -> Result<IndexStats> {
        self.0
            .storage
            .stats(self.status(), self.0.current_file.read().clone())
    }

    pub fn folders(&self) -> Result<Vec<FolderStatus>> {
        self.0.storage.folder_stats(&self.settings().library_paths)
    }

    pub fn status(&self) -> IndexStatus {
        if self.0.paused.load(Ordering::SeqCst) {
            IndexStatus::Paused
        } else if self.0.indexing.load(Ordering::SeqCst) {
            IndexStatus::Indexing
        } else {
            IndexStatus::Idle
        }
    }

    /// The last progress report of the running (or finished) pass.
    pub fn progress(&self) -> Option<IndexProgress> {
        self.0.progress.read().clone()
    }

    pub fn settings(&self) -> AppSettings {
        self.0.settings.read().clone()
    }

    pub(crate) fn persist_settings(&self, settings: AppSettings) -> Result<()> {
        self.0.storage.save_settings(&settings)?;
        *self.0.settings.write() = settings;
        Ok(())
    }

    /// Save settings and re-point the watcher at the configured folders.
    pub fn update_settings(&self, settings: AppSettings) -> Result<()> {
        settings.validate()?;
        self.persist_settings(settings)?;
        indexer::configure_watcher(self)?;
        self.changed();
        Ok(())
    }

    /// What Add folder does. Every error is a sentence the user sees, so it names the path and
    /// what to do. A refused path is never saved.
    pub fn add_folder(&self, path: &str) -> Result<(), String> {
        let canonical = std::fs::canonicalize(path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => format!("{path} does not exist"),
            std::io::ErrorKind::PermissionDenied => {
                format!("Redrob Recall is not allowed to read {path}")
            }
            _ => format!("{path} could not be opened: {error}"),
        })?;
        if !canonical.is_dir() {
            return Err(format!(
                "{path} is a file; choose the folder that contains it"
            ));
        }
        // Being able to resolve a folder is not being able to list it, and an unlistable folder
        // would otherwise be accepted and then index to nothing without a word.
        if let Err(error) = std::fs::read_dir(&canonical) {
            return Err(if error.kind() == std::io::ErrorKind::PermissionDenied {
                format!("Redrob Recall is not allowed to read {path}")
            } else {
                format!("{path} could not be read: {error}")
            });
        }
        let mut settings = self.settings();
        let path = canonical.to_string_lossy().to_string();
        if settings.library_paths.contains(&path) {
            return Err(format!("{path} is already in your library"));
        }
        settings.library_paths.push(path);
        settings.library_paths.sort();
        self.update_settings(settings)
            .map_err(|error| error.to_string())?;
        // A folder that is added must be read, whether or not a pass is running right now.
        indexer::request_index(self);
        Ok(())
    }

    /// Remove a folder from the library and drop what was indexed under it. Refused while a pass is
    /// running, since that pass may be writing records for the folder.
    pub fn remove_folder(&self, path: &str) -> Result<(), String> {
        if !self.set_indexing(true) {
            return Err("Wait for indexing to finish before removing a library folder".into());
        }
        let mut settings = self.settings();
        if !settings
            .library_paths
            .iter()
            .any(|candidate| candidate == path)
        {
            self.set_indexing(false);
            return Err(format!("{path} is not in your library"));
        }
        settings.library_paths.retain(|candidate| candidate != path);
        if let Err(error) = self.persist_settings(settings) {
            self.set_indexing(false);
            self.changed();
            return Err(error.to_string());
        }
        let watcher_result = indexer::configure_watcher(self);
        self.changed();
        let library = self.clone();
        std::thread::Builder::new()
            .name("recall-reconcile".into())
            .spawn(move || {
                if let Err(error) = indexer::run_claimed_full_index(&library) {
                    tracing::error!(%error, "library reconciliation failed after removing a folder");
                }
            })
            .map_err(|error| error.to_string())?;
        watcher_result.map_err(|error| error.to_string())
    }

    /// Start a pass, or queue one behind the running pass. `false` means queued, not ignored.
    pub fn request_index(&self) -> bool {
        indexer::request_index(self)
    }

    pub fn pause_indexing(&self) {
        self.set_paused(true);
    }

    /// Resume after a pause: a new pass picks up where the paused one left off.
    pub fn resume_indexing(&self) {
        self.set_paused(false);
        indexer::request_index(self);
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

    pub(crate) fn set_indexing(&self, value: bool) -> bool {
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

    pub(crate) fn request_rescan(&self) {
        self.0.rescan_requested.store(true, Ordering::SeqCst);
    }

    /// Consume a pending rescan request.
    pub(crate) fn take_rescan_request(&self) -> bool {
        self.0.rescan_requested.swap(false, Ordering::SeqCst)
    }

    pub fn is_indexing(&self) -> bool {
        self.0.indexing.load(Ordering::SeqCst)
    }

    pub(crate) fn set_paused(&self, paused: bool) {
        self.0.paused.store(paused, Ordering::SeqCst);
        self.changed();
    }

    pub fn is_paused(&self) -> bool {
        self.0.paused.load(Ordering::SeqCst)
    }

    pub fn is_shutting_down(&self) -> bool {
        self.0.shutting_down.load(Ordering::SeqCst)
    }

    pub(crate) fn set_current_file(&self, file: Option<String>) {
        *self.0.current_file.write() = file;
    }

    pub(crate) fn set_progress(&self, progress: IndexProgress) {
        *self.0.progress.write() = Some(progress);
        self.changed();
    }

    pub(crate) fn replace_watcher(&self, watcher: notify::RecommendedWatcher) {
        *self.0.watcher.lock() = Some(watcher);
    }

    /// Tell the listener the snapshot may have changed. Best effort, never blocking.
    pub(crate) fn changed(&self) {
        if let Some(on_change) = self.0.on_change.read().as_ref() {
            on_change();
        }
    }

    /// A file inside one of the library folders, canonicalised; anything else is refused, so an
    /// "open" or "reveal" can only point at what the library indexed.
    pub fn validated_source(&self, path: &str) -> Result<PathBuf, String> {
        let source =
            std::fs::canonicalize(path).map_err(|_| "That source file no longer exists")?;
        if !source.is_file() {
            return Err("That source is not a file".into());
        }
        let allowed = self.settings().library_paths.iter().any(|root| {
            std::fs::canonicalize(root)
                .is_ok_and(|canonical_root| source.starts_with(canonical_root))
        });
        if !allowed {
            return Err("That file is outside your selected library folders".into());
        }
        Ok(source)
    }

    /// A consistent copy of the metadata database beside it. Refused while indexing.
    pub fn create_backup(&self) -> Result<PathBuf, String> {
        if self.is_indexing() {
            return Err("Pause indexing before creating a backup".into());
        }
        self.0
            .storage
            .integrity_check()
            .map_err(|error| error.to_string())?;
        self.0
            .storage
            .create_backup()
            .map_err(|error| error.to_string())
    }

    pub fn check_health(&self) -> Result<String, String> {
        self.0
            .storage
            .integrity_check()
            .map_err(|error| error.to_string())?;
        Ok("Local metadata and document references passed their integrity checks".into())
    }

    /// Forget every document and vector. The folders stay configured. Refused while indexing.
    pub fn clear(&self) -> Result<(), String> {
        if self.is_indexing() {
            return Err("Pause indexing before clearing the library".into());
        }
        self.0.storage.clear().map_err(|error| error.to_string())?;
        self.reset_shard().map_err(|error| error.to_string())?;
        self.changed();
        Ok(())
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
            // Another Redrob Recall (the window, or `serve`) holds the index: its WAL lock refuses
            // us with WouldBlock. That is not corruption, and quarantining a healthy index because a
            // second copy started would be the one way to lose it. Refuse instead.
            Err(error) if format!("{error:?}").contains("WouldBlock") => {
                anyhow::bail!(
                    "another Redrob Recall is already using this library ({}); close it first",
                    path.display()
                );
            }
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
