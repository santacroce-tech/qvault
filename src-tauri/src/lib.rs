use tauri::Manager;

pub mod commands;
pub mod document;
pub mod pipeline;
pub mod storage;
pub mod llm;

use std::path::PathBuf;
use std::sync::Arc;
use storage::database::Database;

pub struct AppState {
    pub db: Database,
    pub storage_dir: PathBuf,
    pub llm_state: Arc<llm::LlmState>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Wire up the pure-Rust ONNX backend (tract). Must run before any
    // ort::Session is constructed. CPU-only; works on every target
    // including x86_64-apple-darwin where ort no longer ships prebuilts.
    #[cfg(feature = "llm")]
    ort::set_api(ort_tract::api());

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
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
                llm_state: Arc::new(llm::LlmState::new()),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::document::upload_document,
            commands::document::upload_document_bytes,
            commands::document::scan_document_text,
            commands::document::add_manual_redaction,
            commands::document::get_document,
            commands::document::get_document_text,
            commands::document::get_document_bytes,
            commands::document::extract_docx_text,
            commands::document::get_document_entities,
            commands::document::get_entity_summary,
            commands::document::get_document_pages,
            commands::document::export_clean,
            commands::document::export_clean_flat,
            commands::document::open_file,
            commands::document::delete_document,
            commands::document::reset_app_data,
            commands::redaction::get_redactions,
            commands::redaction::update_redaction,
            commands::redaction::bulk_approve,
            commands::history::get_history,
            commands::history::search_history,
            commands::llm::llm_status,
            commands::llm::llm_download_model,
            commands::llm::llm_load_model,
            commands::llm::llm_set_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
