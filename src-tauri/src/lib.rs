mod commands;
mod embedding;
mod indexer;
mod local_api;
mod models;
mod redrob;
mod search;
mod state;
mod storage;

use state::AppState;
use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "redrob_recall=info".into()),
        )
        .compact()
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let app_state = AppState::initialize(handle.clone())?;
            app.manage(app_state.clone());
            app_state.start_background_services();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::choose_library_folder,
            commands::add_library_path,
            commands::remove_library_path,
            commands::start_indexing,
            commands::pause_indexing,
            commands::search_library,
            commands::ask_library,
            commands::get_documents,
            commands::get_sources,
            commands::open_source,
            commands::reveal_source,
            commands::save_settings,
            commands::connect_with_api_key,
            commands::disconnect_redrob,
            commands::get_connection_status,
            commands::create_library_backup,
            commands::check_library_health,
            commands::clear_library,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Redrob Recall");

    app.run(|app_handle, event| {
        if matches!(event, RunEvent::Exit | RunEvent::ExitRequested { .. }) {
            if let Some(state) = app_handle.try_state::<AppState>() {
                state.shutdown();
            }
        }
    });
}
