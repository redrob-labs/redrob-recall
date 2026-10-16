// SPDX-License-Identifier: Apache-2.0
//! The window's constants and the few OS calls the shell makes: open a file with its app, reveal it
//! in the file manager, open a URL.

use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "Redrob Recall";
pub const APP_ID: &str = "ai.redrob.recall";
pub const LAUNCHER_RELEASES: &str =
    "https://github.com/redrob-labs/redrob-launcher/releases/latest";
pub const REPOSITORY: &str = "https://github.com/redrob-labs/redrob-recall";

/// The main window: 1200 x 780 (eframe shrinks it to the monitor), at least 720 x 480.
pub fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id(APP_ID)
            .with_title(APP_NAME)
            .with_inner_size([1200.0, 780.0])
            .with_min_inner_size([720.0, 480.0])
            .with_icon(window_icon())
            .with_drag_and_drop(true),
        centered: true,
        persistence_path: persistence_path(),
        ..Default::default()
    }
}

/// The 256px icon from the brand pipeline, decoded for the window manager.
fn window_icon() -> egui::IconData {
    const PNG: &[u8] =
        include_bytes!("../../../resources/icons/hicolor/256x256/apps/redrob-recall.png");
    eframe::icon_data::from_png_bytes(PNG).unwrap_or_default()
}

/// Where eframe keeps the window geometry: `~/.redrob/recall/window.ron` beside the library, so the
/// uninstaller's "delete the local index" removes it with the rest.
fn persistence_path() -> Option<PathBuf> {
    redrob_recall_core::default_data_dir()
        .ok()
        .map(|dir| dir.join("window.ron"))
}

pub fn open_path(path: &Path) -> Result<(), String> {
    open::that_detached(path).map_err(|error| error.to_string())
}

/// Show the file selected in the platform's file manager.
pub fn reveal_path(source: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", source.display()))
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(source)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        open::that_detached(source.parent().unwrap_or(source)).map_err(|error| error.to_string())
    }
}

pub fn open_url(url: &str) {
    if let Err(error) = webbrowser::open(url) {
        tracing::warn!(%error, url, "could not open the browser");
    }
}
