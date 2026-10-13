use tauri::Manager;

pub mod commands;
pub mod document;
pub mod entities;
pub mod learning;
pub mod pipeline;
pub mod storage;
pub mod llm;
pub mod ocr;
#[cfg(desktop)]
pub mod qvac;

use std::path::PathBuf;
use std::sync::Arc;
use storage::database::Database;

pub struct AppState {
    pub db: Database,
    pub storage_dir: PathBuf,
    /// OCR runs in progress (cancellation, audit metadata).
    pub ocr_runs: ocr::OcrRuns,
    /// QVAC sidecar + Model Manager (desktop only).
    #[cfg(desktop)]
    pub qvac: Arc<qvac::QvacService>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Logging (stdout + app log dir). Release builds log too so that
            // AI-engine (sidecar) diagnostics reach the app log; nothing in
            // the app logs document content.
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(if cfg!(debug_assertions) { log::LevelFilter::Info } else { log::LevelFilter::Warn })
                    .level_for("app_lib::qvac", log::LevelFilter::Info)
                    .build(),
            )?;

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

            #[cfg(desktop)]
            let qvac = {
                let sidecar = qvac::SidecarPaths::resolve(app.path().resource_dir().ok().as_deref());
                let svc = Arc::new(qvac::QvacService::new(&app_data, None, sidecar));
                svc.start_idle_reaper();
                svc
            };

            app.manage(AppState {
                db,
                storage_dir: app_data,
                ocr_runs: ocr::OcrRuns::default(),
                #[cfg(desktop)]
                qvac,
            });

            // `Qvault --qvac-self-test <models dir>`: exercise the bundled
            // sidecar (OCR on a synthetic image, Layer 3 on a synthetic
            // sentence) with models from <models dir>, print a JSON report
            // and exit. Uses a throw-away cache dir, never user documents.
            #[cfg(desktop)]
            {
                let args: Vec<String> = std::env::args().collect();
                if let Some(pos) = args.iter().position(|a| a == "--qvac-self-test") {
                    let models_dir = args.get(pos + 1).map(PathBuf::from);
                    let handle = app.handle().clone();
                    let res_dir = app.path().resource_dir().ok();
                    std::thread::spawn(move || {
                        let tmp = std::env::temp_dir().join(format!("qvault-selftest-{}", uuid::Uuid::new_v4()));
                        let _ = std::fs::create_dir_all(&tmp);
                        let sidecar = qvac::SidecarPaths::resolve(res_dir.as_deref());
                        let img = res_dir.map(|d| d.join("selftest").join("ocr-sample.png")).filter(|p| p.is_file());
                        let svc = Arc::new(qvac::QvacService::new(&tmp, models_dir.as_deref(), sidecar));
                        let rep = svc.self_test(img.as_deref());
                        println!("{}", serde_json::to_string_pretty(&rep).unwrap_or_default());
                        svc.shutdown();
                        let _ = std::fs::remove_dir_all(&tmp);
                        let ok = rep.error.is_none() && rep.ocr_error.is_none() && rep.layer3_error.is_none() && rep.embedding_error.is_none();
                        handle.exit(if ok { 0 } else { 1 });
                    });
                }
            }

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
            commands::document::export_redacted_docx,
            commands::document::export_redacted_pdf,
            commands::document::open_file,
            commands::document::delete_document,
            commands::document::reset_app_data,
            commands::redaction::get_redactions,
            commands::redaction::update_redaction,
            commands::redaction::bulk_approve,
            commands::history::get_history,
            commands::history::search_history,
            commands::ocr::classify_pdf_pages,
            commands::ocr::ocr_status,
            commands::ocr::ocr_finish,
            commands::ocr::get_ocr_pages,
            #[cfg(desktop)]
            commands::ocr::ocr_page,
            #[cfg(desktop)]
            commands::ocr::ocr_cancel,
            #[cfg(desktop)]
            commands::ocr::ai_set_ocr_dpi,
            #[cfg(desktop)]
            commands::ai::ai_capabilities,
            #[cfg(desktop)]
            commands::ai::ai_list_models,
            #[cfg(desktop)]
            commands::ai::ai_download_model,
            #[cfg(desktop)]
            commands::ai::ai_cancel_download,
            #[cfg(desktop)]
            commands::ai::ai_import_model,
            #[cfg(desktop)]
            commands::ai::ai_delete_model,
            #[cfg(desktop)]
            commands::ai::ai_assess_fit,
            #[cfg(desktop)]
            commands::ai::ai_set_layer3,
            #[cfg(desktop)]
            commands::ai::ai_self_test,
            commands::ai::ai_open_manual_download,
            #[cfg(desktop)]
            commands::ai::ai_set_entity_embeddings,
            commands::entities::regroup_entities,
            commands::entities::get_entity_groups,
            commands::entities::get_entity_proposals,
            commands::entities::get_entity_group_documents,
            commands::entities::merge_entity_groups,
            commands::entities::split_entity_variant,
            commands::entities::dismiss_entity_proposal,
            commands::entities::get_redaction_notes,
            commands::learning::get_document_learning,
            commands::learning::reject_previously_rejected,
            commands::learning::restore_suppressed_finding,
            commands::learning::get_not_personal_data,
            commands::learning::add_not_personal_data,
            commands::learning::remove_not_personal_data,
            commands::learning::export_not_personal_data,
            commands::learning::preview_not_personal_data_import,
            commands::learning::apply_not_personal_data_import,
            commands::learning::get_not_personal_data_candidates,
            commands::learning::get_learning_summary,
            commands::learning::clear_review_memory,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_handle, _event| {
        // Stop the QVAC sidecar cleanly (shutdown request, kill after 2 s).
        #[cfg(desktop)]
        if let tauri::RunEvent::Exit = _event {
            if let Some(state) = _handle.try_state::<AppState>() {
                state.qvac.shutdown();
            }
        }
    });
}
