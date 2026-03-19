use std::path::Path;
use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use crate::pipeline;
use crate::storage::database::DocumentRow;
use crate::storage::encryption;
use crate::AppState;

/// Upload a document: check license, encrypt + store. Does NOT extract text or scan for PII.
/// The frontend will extract text via PDF.js and call scan_document_text.
#[tauri::command]
pub async fn upload_document(
    file_path: String,
    page_count: i32,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Check license (yearly license = unlimited documents)
    let licensed = state
        .db
        .is_licensed()
        .map_err(|e| format!("License check failed: {}", e))?;

    if !licensed {
        return Err("No active license. Please activate a yearly license to use Qvault.".to_string());
    }

    let source = Path::new(&file_path);
    let filename = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext != "pdf" && ext != "docx" {
        return Err("Unsupported file type. Please upload a PDF or DOCX.".to_string());
    }

    let doc_id = Uuid::new_v4().to_string();

    // Compute file hash
    let file_data = std::fs::read(source)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    let hash = ring::digest::digest(&ring::digest::SHA256, &file_data);
    let file_hash = hex::encode(hash.as_ref());

    // Copy to encrypted storage
    let storage_dir = state.storage_dir.join("documents");
    std::fs::create_dir_all(&storage_dir)
        .map_err(|e| format!("Failed to create storage dir: {}", e))?;

    let encrypted_path = storage_dir.join(format!("{}.enc", doc_id));
    encryption::encrypt_file(source, &encrypted_path)?;

    // Store in database (status = 'processing', PII scan happens next via scan_document_text)
    state
        .db
        .insert_document(
            &doc_id,
            &filename,
            &ext,
            &file_hash,
            page_count,
            encrypted_path.to_str().unwrap_or(""),
        )
        .map_err(|e| format!("Database error: {}", e))?;

    // Audit log
    let _ = state.db.insert_audit_log(
        Some(&doc_id),
        "upload",
        Some(&format!("Uploaded {}", filename)),
    );

    Ok(doc_id)
}

/// Receive text extracted by PDF.js on the frontend, run PII detection, store results.
/// Now also stores: entities in the knowledge base, page text for reconstruction, and cleaned text.
#[tauri::command]
pub async fn scan_document_text(
    id: String,
    pages: Vec<String>,
    state: State<'_, AppState>,
) -> Result<i32, String> {
    let jurisdictions = vec!["global", "us", "eu", "br", "de"];
    let mut total_pii = 0;

    // Store text and run PII pipeline per page
    let text_dir = state.storage_dir.join("text");
    std::fs::create_dir_all(&text_dir)
        .map_err(|e| format!("Failed to create text dir: {}", e))?;

    for (page_num, text) in pages.iter().enumerate() {
        // Save text to filesystem (backward compat)
        let text_path = text_dir.join(format!("{}_{}.txt", id, page_num));
        std::fs::write(&text_path, text)
            .map_err(|e| format!("Failed to write text: {}", e))?;

        // Store raw page text in database for reconstruction
        state
            .db
            .insert_document_page(&id, page_num as i32, text)
            .map_err(|e| format!("Failed to store page text: {}", e))?;

        // Run PII pipeline
        let entities = pipeline::run_pipeline(text, page_num as u32, &jurisdictions);

        // Build cleaned text by applying redactions
        let mut cleaned = text.clone();
        // Apply replacements in reverse order to preserve offsets
        let mut sorted_entities = entities.clone();
        sorted_entities.sort_by(|a, b| b.start.cmp(&a.start));
        for entity in &sorted_entities {
            if entity.start < cleaned.len() && entity.end <= cleaned.len() {
                cleaned.replace_range(entity.start..entity.end, &entity.replacement);
            }
        }

        // Store cleaned text
        let _ = state.db.update_page_cleaned_text(&id, page_num as i32, &cleaned);

        for entity in &entities {
            // Store redaction (for review workflow)
            state
                .db
                .insert_redaction(
                    &entity.id,
                    &id,
                    page_num as i32,
                    entity.start as i32,
                    entity.end as i32,
                    &entity.text,
                    &entity.replacement,
                    &entity.category.to_string(),
                    &entity.layer,
                    entity.confidence,
                )
                .map_err(|e| format!("Failed to store redaction: {}", e))?;

            // Store entity in knowledge base (normalized for cross-document search)
            let normalized = entity.text.trim().to_string();
            let entity_id = Uuid::new_v4().to_string();
            state
                .db
                .insert_entity(
                    &entity_id,
                    &id,
                    &entity.text,
                    &normalized,
                    &entity.category.to_string(),
                    page_num as i32,
                    entity.start as i32,
                    entity.end as i32,
                    entity.confidence,
                    &entity.layer,
                )
                .map_err(|e| format!("Failed to store entity: {}", e))?;
        }

        total_pii += entities.len();
    }

    // Update document page count and status
    state
        .db
        .update_document_page_count(&id, pages.len() as i32)
        .map_err(|e| format!("Failed to update page count: {}", e))?;

    state
        .db
        .update_document_status(&id, "review", total_pii as i32)
        .map_err(|e| format!("Failed to update document: {}", e))?;

    // Audit log
    let _ = state.db.insert_audit_log(
        Some(&id),
        "scan_complete",
        Some(&format!("Found {} PII entities across {} pages", total_pii, pages.len())),
    );

    Ok(total_pii as i32)
}

/// Manually add a redaction (for advanced mode — user selects text)
#[tauri::command]
pub async fn add_manual_redaction(
    document_id: String,
    page: i32,
    start_offset: i32,
    end_offset: i32,
    original_text: String,
    replacement: String,
    category: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let redaction_id = Uuid::new_v4().to_string();

    state
        .db
        .insert_redaction(
            &redaction_id,
            &document_id,
            page,
            start_offset,
            end_offset,
            &original_text,
            &replacement,
            &category,
            "manual",
            1.0,
        )
        .map_err(|e| format!("Failed to add redaction: {}", e))?;

    // Also store as entity for cross-document tracking
    let entity_id = Uuid::new_v4().to_string();
    let _ = state.db.insert_entity(
        &entity_id,
        &document_id,
        &original_text,
        &original_text.trim().to_string(),
        &category,
        page,
        start_offset,
        end_offset,
        1.0,
        "manual",
    );

    // Update PII count
    let redactions = state
        .db
        .get_redactions(&document_id)
        .map_err(|e| format!("DB error: {}", e))?;
    state
        .db
        .update_document_status(&document_id, "review", redactions.len() as i32)
        .map_err(|e| format!("DB error: {}", e))?;

    Ok(redaction_id)
}

#[tauri::command]
pub async fn get_document(
    id: String,
    state: State<'_, AppState>,
) -> Result<DocumentRow, String> {
    state
        .db
        .get_document(&id)
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "Document not found".to_string())
}

#[tauri::command]
pub async fn get_document_text(
    id: String,
    page: u32,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let text_path = state
        .storage_dir
        .join("text")
        .join(format!("{}_{}.txt", id, page));

    std::fs::read_to_string(&text_path)
        .map_err(|e| format!("Failed to read page text: {}", e))
}

/// Returns raw PDF bytes as base64 for rendering in the frontend
#[tauri::command]
pub async fn get_document_bytes(
    id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let doc = state
        .db
        .get_document(&id)
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "Document not found".to_string())?;

    use base64::Engine;
    let decrypted = encryption::decrypt_file(Path::new(&doc.original_path))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&decrypted))
}

/// Get entities extracted from a document (knowledge base entries)
#[tauri::command]
pub async fn get_document_entities(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::storage::database::EntityRow>, String> {
    state
        .db
        .get_entities_by_document(&document_id)
        .map_err(|e| format!("Database error: {}", e))
}

/// Get all unique entities across all documents with occurrence counts
#[tauri::command]
pub async fn get_entity_summary(
    state: State<'_, AppState>,
) -> Result<Vec<crate::storage::database::EntitySummary>, String> {
    state
        .db
        .get_entity_summary()
        .map_err(|e| format!("Database error: {}", e))
}

/// Get stored page text (raw + cleaned) for a document
#[tauri::command]
pub async fn get_document_pages(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::storage::database::DocumentPageRow>, String> {
    state
        .db
        .get_document_pages(&document_id)
        .map_err(|e| format!("Database error: {}", e))
}

#[derive(Debug, Deserialize)]
pub struct RedactionRect {
    pub page: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[tauri::command]
pub async fn export_clean(
    id: String,
    redaction_rects: Vec<RedactionRect>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let doc = state
        .db
        .get_document(&id)
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "Document not found".to_string())?;

    let pdf_bytes = encryption::decrypt_file(Path::new(&doc.original_path))?;

    let clean_dir = state.storage_dir.join("clean");
    std::fs::create_dir_all(&clean_dir)
        .map_err(|e| format!("Failed to create clean dir: {}", e))?;

    let clean_filename = format!(
        "{}_redacted.pdf",
        doc.filename.rsplit_once('.').map(|(n, _)| n).unwrap_or(&doc.filename)
    );
    let clean_path = clean_dir.join(&clean_filename);

    crate::document::redactor::export_redacted_pdf(&pdf_bytes, &redaction_rects, &clean_path)?;

    state
        .db
        .update_document_clean_path(&id, clean_path.to_str().unwrap_or(""))
        .map_err(|e| format!("Database error: {}", e))?;

    // Audit log
    let _ = state.db.insert_audit_log(
        Some(&id),
        "export",
        Some(&format!("Exported redacted version: {}", clean_filename)),
    );

    Ok(clean_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn reset_app_data(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let text_dir = state.storage_dir.join("text");
    let _ = std::fs::remove_dir_all(&text_dir);
    let clean_dir = state.storage_dir.join("clean");
    let _ = std::fs::remove_dir_all(&clean_dir);
    let doc_dir = state.storage_dir.join("documents");
    let _ = std::fs::remove_dir_all(&doc_dir);

    state.db.with_conn(|conn| {
        conn.execute_batch("
            DELETE FROM entities;
            DELETE FROM document_pages;
            DELETE FROM redactions;
            DELETE FROM documents;
            DELETE FROM audit_log;
        ")
    }).map_err(|e| format!("DB reset failed: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn delete_document(
    id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let doc = state.db.get_document(&id).map_err(|e| format!("DB error: {}", e))?;

    if let Some(doc) = doc {
        let _ = std::fs::remove_file(&doc.original_path);
        if let Some(clean) = &doc.clean_path {
            let _ = std::fs::remove_file(clean);
        }
        for i in 0..doc.page_count {
            let text_path = state
                .storage_dir
                .join("text")
                .join(format!("{}_{}.txt", id, i));
            let _ = std::fs::remove_file(text_path);
        }
    }

    state
        .db
        .delete_document(&id)
        .map_err(|e| format!("Database error: {}", e))?;

    Ok(())
}
