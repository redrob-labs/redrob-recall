// SPDX-License-Identifier: Apache-2.0
//! The Redrob account this app spends: its own key when it has one, else the computer's shared
//! sign-in written by Redrob Launcher (redrob-launcher `docs/shared-account.md`). Read fresh every
//! time it is needed -- the person may sign in or out in the launcher while this window is open --
//! and never written, logged or displayed beyond `accountName` and `apiKeyName`.

use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const CONSOLE_API: &str = "https://console.redrob.ai/api/backend/v1";
/// Overrides the shared account file's directory (Redrob Launcher's `REDROB_SHARED_DIR`).
pub const SHARED_DIR_ENV: &str = "REDROB_SHARED_DIR";
/// A key exported in a terminal wins over every file (the contract's rule 4).
pub const OWN_KEY_ENV: &str = "REDROB_API_KEY";
/// `REDROB_CONSOLE_API` replaces the base URL (staging).
pub const CONSOLE_API_ENV: &str = "REDROB_CONSOLE_API";

/// `{apiKey, apiKeyName, accountName, apiBaseUrl}`: the shape the launcher writes and the apps read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stored {
    pub api_key: String,
    pub api_key_name: String,
    pub account_name: String,
    pub api_base_url: String,
}

/// Where a usable key came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `REDROB_API_KEY` in the environment.
    Environment,
    /// This app's own `account.json` in its data directory.
    Own,
    /// Redrob Launcher's shared `account.json`.
    Shared,
}

/// A key and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub stored: Stored,
    pub source: Source,
}

impl Account {
    /// The console base URL the key is spent at: the file's, else the environment override, else
    /// production.
    pub fn api_base_url(&self) -> String {
        let from_file = self.stored.api_base_url.trim();
        if !from_file.is_empty() {
            return from_file.trim_end_matches('/').to_string();
        }
        console_base()
    }

    /// What the Settings screen shows: never the key.
    pub fn label(&self) -> String {
        let name = if self.stored.account_name.trim().is_empty() {
            "Redrob workspace".to_string()
        } else {
            self.stored.account_name.trim().to_string()
        };
        match self.source {
            Source::Environment => format!("{name} (REDROB_API_KEY)"),
            Source::Own => format!("{name} (this app's key)"),
            Source::Shared => {
                let key_name = self.stored.api_key_name.trim();
                if key_name.is_empty() {
                    format!("{name} (Redrob Launcher)")
                } else {
                    format!("{name} ({key_name}, Redrob Launcher)")
                }
            }
        }
    }
}

pub fn console_base() -> String {
    std::env::var(CONSOLE_API_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .unwrap_or_else(|| CONSOLE_API.to_string())
}

/// Redrob Launcher's shared account file for this platform.
pub fn shared_account_path() -> Option<PathBuf> {
    let env = |key: &str| std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if let Some(dir) = env(SHARED_DIR_ENV) {
        dir
    } else if cfg!(windows) {
        env("APPDATA")?.join("Redrob").join("Shared")
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support/ai.redrob.shared")
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|home| home.join(".config")))?
            .join("redrob-shared")
    };
    Some(dir.join("account.json"))
}

/// This app's own `account.json`, beside the library (`~/.redrob/recall/account.json`). Nothing in
/// this app writes it; a person or another tool may.
pub fn own_account_path() -> Option<PathBuf> {
    redrob_recall_core::default_data_dir()
        .ok()
        .map(|dir| dir.join("account.json"))
}

fn load(path: &std::path::Path) -> Option<Stored> {
    let text = std::fs::read_to_string(path).ok()?;
    let stored: Stored = serde_json::from_str(&text).ok()?;
    (!stored.api_key.trim().is_empty()).then(|| Stored {
        api_key: stored.api_key.trim().to_string(),
        api_key_name: stored.api_key_name,
        account_name: stored.account_name,
        api_base_url: stored.api_base_url,
    })
}

/// The account to use right now, by the contract's precedence, or `None` for "not signed in".
pub fn current() -> Option<Account> {
    if let Some(key) = std::env::var(OWN_KEY_ENV)
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
    {
        return Some(Account {
            stored: Stored {
                api_key: key,
                ..Stored::default()
            },
            source: Source::Environment,
        });
    }
    if let Some(stored) = own_account_path().as_deref().and_then(load) {
        return Some(Account {
            stored,
            source: Source::Own,
        });
    }
    shared_account_path()
        .as_deref()
        .and_then(load)
        .map(|stored| Account {
            stored,
            source: Source::Shared,
        })
}

/// Watches the shared and the own `account.json` for a sign-in or sign-out while the window is
/// open (the contract's rule 2: read when needed, not once at start-up). The directories are
/// watched, not the files, because the launcher writes through a temporary file renamed over the
/// old one, and a watch on the old inode would go stale after the first rename.
pub struct Watcher {
    changed: Arc<AtomicBool>,
    watcher: Option<notify::RecommendedWatcher>,
}

impl Watcher {
    /// Start watching; `wake` is called from the watcher thread when a file changed (the shell asks
    /// egui to repaint from it). Directories that do not exist yet are retried at every `take`.
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let changed = Arc::new(AtomicBool::new(false));
        let flag = changed.clone();
        let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            let Ok(event) = result else { return };
            let touches_account = event.paths.iter().any(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("account"))
            });
            if touches_account || event.paths.is_empty() {
                flag.store(true, Ordering::Relaxed);
                wake();
            }
        })
        .ok();
        let mut this = Self { changed, watcher };
        this.watch_dirs();
        this
    }

    fn watch_dirs(&mut self) {
        use notify::Watcher as _;
        let Some(watcher) = self.watcher.as_mut() else { return };
        for path in [shared_account_path(), own_account_path()].into_iter().flatten() {
            if let Some(dir) = path.parent().filter(|dir| dir.is_dir()) {
                // Watching the same directory twice is refused harmlessly on every backend.
                let _ = watcher.watch(dir, notify::RecursiveMode::NonRecursive);
            }
        }
    }

    /// Whether a change was seen since the last call. Also re-arms the watch on directories that
    /// have appeared since (the launcher creates the shared folder at the first sign-in).
    pub fn take(&mut self) -> bool {
        let changed = self.changed.swap(false, Ordering::Relaxed);
        if !changed {
            return false;
        }
        self.watch_dirs();
        true
    }

    /// Re-arm directory watches (for a slow timer, when a directory may have been created).
    pub fn rearm(&mut self) {
        self.watch_dirs();
    }
}
