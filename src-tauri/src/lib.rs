use tauri::Manager;

pub mod commands;
pub mod document;
pub mod pipeline;
pub mod storage;

use std::path::PathBuf;
use storage::database::Database;

pub struct AppState {
    pub db: Database,
    pub storage_dir: PathBuf,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Set up storage directory
            let app_data = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");
            std::fs::create_dir_all(&app_data)
                .expect("Failed to create app data directory");

            // Initialize database
            let db_path = app_data.join("qvault.db");
            let db = Database::open(&db_path)
                .expect("Failed to initialize database");

            app.manage(AppState {
                db,
                storage_dir: app_data,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::document::upload_document,
            commands::document::scan_document_text,
            commands::document::add_manual_redaction,
            commands::document::get_document,
            commands::document::get_document_text,
            commands::document::get_document_bytes,
            commands::document::export_clean,
            commands::document::delete_document,
            commands::document::reset_app_data,
            commands::redaction::get_redactions,
            commands::redaction::update_redaction,
            commands::redaction::bulk_approve,
            commands::history::get_history,
            commands::history::search_history,
            commands::license::get_credits,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
