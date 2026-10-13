//! Redrob Launcher's shared sign-in, read by this app as a fallback.
//!
//! The launcher signs a computer in to a Redrob workspace once and writes the workspace key to
//! one file every Redrob app may read (redrob-launcher `docs/shared-account.md`). This module
//! only reads that file, and reads it fresh each time it is asked, because the person may sign
//! in or out of the launcher while this app is open. The key this app stores in the OS keyring
//! always wins; the shared file is used only when there is no key of this app's own.
//!
//! A missing file, unreadable JSON or an empty `apiKey` all mean "not signed in". The key is
//! never logged.

use serde::Deserialize;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Overrides the shared account file's directory on every platform (tests, portable setups).
pub const SHARED_DIR_ENV: &str = "REDROB_SHARED_DIR";

/// The name of the key the launcher creates, shown instead of "This device".
pub const LAUNCHER_KEY_NAME: &str = "Redrob Launcher";

/// What the launcher wrote. Only `api_key` is secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedAccount {
    pub api_key: String,
    pub api_key_name: String,
    pub account_name: String,
    pub api_base_url: String,
}

/// The file as written; every field is optional at the boundary.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    api_key_name: String,
    #[serde(default)]
    account_name: String,
    #[serde(default)]
    api_base_url: String,
}

/// Where the launcher keeps the shared account on this computer, or `None` when no home can
/// be found. Resolved from the real environment.
pub fn shared_account_path() -> Option<PathBuf> {
    shared_account_path_from(|key| std::env::var_os(key))
}

/// `shared_account_path` over any environment, so the platform rules can be tested without
/// touching the process environment.
pub fn shared_account_path_from(env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let env = |key: &str| env(key).filter(|v| !v.is_empty()).map(PathBuf::from);
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

/// The launcher's sign-in right now, or `None` when there is none. Read-only, read fresh.
pub fn load(path: &Path) -> Option<SharedAccount> {
    let bytes = std::fs::read(path).ok()?;
    let stored: Stored = serde_json::from_slice(&bytes).ok()?;
    let api_key = stored.api_key.trim().to_string();
    if api_key.is_empty() {
        return None;
    }
    Some(SharedAccount {
        api_key,
        api_key_name: if stored.api_key_name.trim().is_empty() {
            LAUNCHER_KEY_NAME.to_string()
        } else {
            stored.api_key_name.trim().to_string()
        },
        account_name: stored.account_name.trim().to_string(),
        api_base_url: stored.api_base_url.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn the_override_directory_wins_on_every_platform() {
        let path = shared_account_path_from(env(&[
            (SHARED_DIR_ENV, "/portable/shared"),
            ("HOME", "/home/someone"),
            ("APPDATA", "C:\\Users\\someone\\AppData\\Roaming"),
        ]));
        assert_eq!(
            path,
            Some(PathBuf::from("/portable/shared").join("account.json"))
        );
        // An empty override is no override.
        let path = shared_account_path_from(env(&[(SHARED_DIR_ENV, ""), ("HOME", "/h")]));
        assert_ne!(path, Some(PathBuf::from("account.json")));
        assert!(path.is_some());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_uses_xdg_config_home_then_dot_config() {
        assert_eq!(
            shared_account_path_from(env(&[("XDG_CONFIG_HOME", "/xdg"), ("HOME", "/h")])),
            Some(PathBuf::from("/xdg/redrob-shared/account.json"))
        );
        assert_eq!(
            shared_account_path_from(env(&[("HOME", "/h")])),
            Some(PathBuf::from("/h/.config/redrob-shared/account.json"))
        );
        assert_eq!(shared_account_path_from(env(&[])), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_uses_application_support() {
        assert_eq!(
            shared_account_path_from(env(&[("HOME", "/Users/me")])),
            Some(PathBuf::from(
                "/Users/me/Library/Application Support/ai.redrob.shared/account.json"
            ))
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_uses_appdata() {
        assert_eq!(
            shared_account_path_from(env(&[("APPDATA", "C:\\Users\\me\\AppData\\Roaming")])),
            Some(PathBuf::from(
                "C:\\Users\\me\\AppData\\Roaming\\Redrob\\Shared\\account.json"
            ))
        );
    }

    #[test]
    fn missing_malformed_and_empty_files_mean_not_signed_in() {
        let dir =
            std::env::temp_dir().join(format!("redrob-recall-shared-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("account.json");
        assert_eq!(load(&path), None, "missing file");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), None, "malformed json");
        std::fs::write(&path, r#"{"apiKey": "   "}"#).unwrap();
        assert_eq!(load(&path), None, "blank key");
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(load(&path), None, "no key");
        std::fs::write(
            &path,
            r#"{"apiKey":"rk_shared_0123456789","apiKeyName":"Redrob Launcher","accountName":"Acme","apiBaseUrl":"https://c/v1"}"#,
        )
        .unwrap();
        assert_eq!(
            load(&path),
            Some(SharedAccount {
                api_key: "rk_shared_0123456789".into(),
                api_key_name: "Redrob Launcher".into(),
                account_name: "Acme".into(),
                api_base_url: "https://c/v1".into(),
            })
        );
        // A file with only the key still names the launcher.
        std::fs::write(&path, r#"{"apiKey":"rk_only"}"#).unwrap();
        assert_eq!(
            load(&path).map(|a| a.api_key_name),
            Some(LAUNCHER_KEY_NAME.to_string())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
