// SPDX-License-Identifier: Apache-2.0
//! The window's own preferences -- the interface language and the theme -- in `ui.json` beside the
//! library (`~/.redrob/recall/ui.json`), so the uninstaller's "delete the local index" removes them
//! with the rest. Not in the library's settings: those are the memory layer's and travel over the
//! local API, and a client has no business reading which theme the window is drawn in.

use crate::theme::ThemePref;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    /// `auto`, or a language code from `i18n::LANGUAGES`.
    pub language: String,
    pub theme: ThemePref,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            language: crate::i18n::AUTO.to_owned(),
            theme: ThemePref::System,
        }
    }
}

fn path() -> Option<PathBuf> {
    redrob_recall_core::default_data_dir()
        .ok()
        .map(|dir| dir.join("ui.json"))
}

impl UiPrefs {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(self) {
            Ok(text) => {
                if let Err(error) = std::fs::write(&path, text + "\n") {
                    tracing::warn!(%error, path = %path.display(), "could not save the UI preferences");
                }
            }
            Err(error) => tracing::warn!(%error, "could not serialise the UI preferences"),
        }
    }
}
