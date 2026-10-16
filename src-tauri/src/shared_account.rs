//! Redrob Launcher's shared sign-in, read by this app (redrob-launcher `docs/shared-account.md`).
//!
//! The launcher signs a computer in once and writes `account.json`; every Redrob app reads it so
//! that sign-in counts as its own. This app reads the file when it needs it and watches the
//! directory, so a sign-in or sign-out in the launcher shows up here without a restart. The file is
//! never written from here, and the key is never logged, shown or copied: `Debug` names the
//! account and the key's name only.
//!
//! Paths: `REDROB_SHARED_DIR` when set; otherwise `$XDG_CONFIG_HOME/redrob-shared` on Linux,
//! `~/Library/Application Support/ai.redrob.shared` on macOS, `%APPDATA%\Redrob\Shared` on Windows.

use serde::Deserialize;
use std::{
    fmt,
    path::{Path, PathBuf},
};

pub const FILE_NAME: &str = "account.json";

/// Where the key is spent when the file names no base URL (the brief's console base).
pub const DEFAULT_API_BASE_URL: &str = "https://console.redrob.ai/api/backend/v1";

/// The file's contents. Only `api_key` is secret.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SharedAccount {
    pub api_key: String,
    #[serde(default)]
    pub api_key_name: Option<String>,
    #[serde(default)]
    pub account_name: Option<String>,
    #[serde(default = "default_api_base_url")]
    pub api_base_url: String,
}

fn default_api_base_url() -> String {
    DEFAULT_API_BASE_URL.to_string()
}

impl fmt::Debug for SharedAccount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedAccount")
            .field("api_key_name", &self.api_key_name)
            .field("account_name", &self.account_name)
            .field("api_base_url", &self.api_base_url)
            .finish_non_exhaustive()
    }
}

impl SharedAccount {
    /// What the connection panel shows: the account, then the key's name, never the key.
    pub fn label(&self) -> Option<String> {
        self.account_name
            .as_deref()
            .or(self.api_key_name.as_deref())
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
    }

    /// `<apiBaseUrl>/<path>` without a doubled slash.
    pub fn endpoint(&self, path: &str) -> String {
        format!("{}/{path}", self.api_base_url.trim_end_matches('/'))
    }
}

/// The directory the launcher writes to on this platform, or `None` when the home directory is
/// unknown. `REDROB_SHARED_DIR` wins everywhere.
pub fn shared_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("REDROB_SHARED_DIR").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .filter(|dir| !dir.is_empty())
            .map(|dir| PathBuf::from(dir).join("Redrob").join("Shared"))
    }
    #[cfg(target_os = "macos")]
    {
        directories::BaseDirs::new().map(|base| {
            base.home_dir()
                .join("Library")
                .join("Application Support")
                .join("ai.redrob.shared")
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        directories::BaseDirs::new().map(|base| base.config_dir().join("redrob-shared"))
    }
}

pub fn account_path() -> Option<PathBuf> {
    shared_dir().map(|dir| dir.join(FILE_NAME))
}

/// The shared sign-in right now. A missing file, unreadable JSON, or an empty key all mean "not
/// signed in", as the contract says; none of them is an error worth more than a debug line.
pub fn read() -> Option<SharedAccount> {
    read_at(&account_path()?)
}

pub fn read_at(path: &Path) -> Option<SharedAccount> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!(%error, path = %path.display(), "shared account file is unreadable");
            }
            return None;
        }
    };
    let account: SharedAccount = match serde_json::from_slice(&bytes) {
        Ok(account) => account,
        Err(error) => {
            tracing::debug!(%error, path = %path.display(), "shared account file is not valid JSON");
            return None;
        }
    };
    if account.api_key.trim().is_empty() {
        return None;
    }
    let api_base_url = account.api_base_url.trim();
    Some(SharedAccount {
        api_key: account.api_key.trim().to_string(),
        api_base_url: if api_base_url.is_empty() {
            DEFAULT_API_BASE_URL.to_string()
        } else {
            api_base_url.to_string()
        },
        ..account
    })
}

/// Watches the shared directory and calls `on_change` whenever `account.json` is created, replaced
/// or removed. The launcher writes through a rename, which arrives as create/modify/remove events
/// on the directory, so the directory is watched rather than the file. The directory may not
/// exist yet (never signed in): then the parent is watched so the first sign-in is still seen.
/// Returns the watcher, which stops when dropped.
pub fn watch(
    on_change: impl Fn() + Send + 'static,
) -> Result<Option<notify::RecommendedWatcher>, notify::Error> {
    use notify::{RecursiveMode, Watcher};
    let Some(dir) = shared_dir() else {
        return Ok(None);
    };
    let target = if dir.is_dir() {
        dir.clone()
    } else {
        match dir.parent() {
            Some(parent) if parent.is_dir() => parent.to_path_buf(),
            _ => return Ok(None),
        }
    };
    let watched_dir = dir.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let Ok(event) = result else { return };
        if !(event.kind.is_create() || event.kind.is_modify() || event.kind.is_remove()) {
            return;
        }
        let concerns_account = event.paths.iter().any(|path| {
            path.file_name().is_some_and(|name| name == FILE_NAME)
                || path.ends_with(&watched_dir)
                || path.starts_with(&watched_dir)
        });
        if concerns_account {
            on_change();
        }
    })?;
    watcher.watch(&target, RecursiveMode::NonRecursive)?;
    Ok(Some(watcher))
}
