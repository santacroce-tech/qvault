use std::path::Path;
use serde::Deserialize;
use tauri::{Emitter, State};
use uuid::Uuid;

use crate::pipeline;
use crate::storage::database::DocumentRow;
use crate::storage::encryption;
use crate::AppState;

/// Shared logic for processing an uploaded document: validate, encrypt, store in DB.
/// `source` is the path to the file on disk (original or temp file from bytes upload).
fn process_upload(
    source: &Path,
    filename: &str,
    page_count: i32,
    state: &AppState,
) -> Result<String, String> {
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Standalone images are converted to an image-only PDF on import, so the
    // viewer, OCR, export and verification treat them like scanned PDFs.
    let image_kind = crate::ocr::image_import::ImageKind::from_extension(&ext);
    if ext != "pdf" && ext != "docx" && image_kind.is_none() {
        return Err("Unsupported file type. Please upload a PDF, DOCX, PNG, JPEG or TIFF file.".to_string());
    }

    let doc_id = Uuid::new_v4().to_string();

    // Compute file hash (of the file as uploaded)
    let file_data = std::fs::read(source)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    let hash = ring::digest::digest(&ring::digest::SHA256, &file_data);
    let file_hash = hex::encode(hash.as_ref());

    // Copy to encrypted storage
    let storage_dir = state.storage_dir.join("documents");
    std::fs::create_dir_all(&storage_dir)
        .map_err(|e| format!("Failed to create storage dir: {}", e))?;

    let encrypted_path = storage_dir.join(format!("{}.enc", doc_id));
    let (stored_type, page_count, note) = match image_kind {
        Some(kind) => {
            let converted = crate::ocr::image_import::image_to_pdf(&file_data, kind)?;
            let tmp = storage_dir.join(format!("{}.import.pdf", doc_id));
            std::fs::write(&tmp, &converted.pdf)
                .map_err(|e| format!("Failed to write converted image: {}", e))?;
            let r = encryption::encrypt_file(&tmp, &encrypted_path);
            let _ = std::fs::remove_file(&tmp);
            r?;
            (
                "pdf".to_string(),
                converted.pages as i32,
                format!(" (converted from {} to an image-only PDF, {} page(s))", kind.name(), converted.pages),
            )
        }
        None => {
            encryption::encrypt_file(source, &encrypted_path)?;
            (ext.clone(), page_count, String::new())
        }
    };
    drop(file_data);

    // Store in database (status = 'processing', PII scan happens next via scan_document_text)
    state
        .db
        .insert_document(
            &doc_id,
            filename,
            &stored_type,
            &file_hash,
            page_count,
            encrypted_path.to_str().unwrap_or(""),
        )
        .map_err(|e| format!("Database error: {}", e))?;

    // Audit log
    let _ = state.db.insert_audit_log(
        Some(&doc_id),
        "upload",
        Some(&format!("Uploaded {}{}", filename, note)),
    );

    Ok(doc_id)
}

/// Upload a document: encrypt + store. Does NOT extract text or scan for PII.
/// The frontend will extract text via PDF.js and call scan_document_text.
#[tauri::command]
pub async fn upload_document(
    file_path: String,
    page_count: i32,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let source = Path::new(&file_path);
    let filename = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    process_upload(source, &filename, page_count, &state)
}

/// Upload a document from raw bytes (base64-encoded).
/// Used as a mobile fallback when native file dialogs are unavailable.
#[tauri::command]
pub async fn upload_document_bytes(
    filename: String,
    data: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use base64::Engine;
    let file_bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("Failed to decode base64 data: {}", e))?;

    // Write to a temp file in the storage dir so encryption can read from disk
    let tmp_path = state.storage_dir.join(format!("upload_{}", filename));
    std::fs::write(&tmp_path, &file_bytes)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;

    let result = process_upload(&tmp_path, &filename, 0, &state);

    // Clean up temp file
    let _ = std::fs::remove_file(&tmp_path);

    result
}

/// Receive text extracted by PDF.js on the frontend, run PII detection, store results.
/// Now also stores: entities in the knowledge base, page text for reconstruction, and cleaned text.
#[tauri::command]
pub async fn scan_document_text(
    id: String,
    pages: Vec<String>,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<i32, String> {
    let jurisdictions = vec!["global", "us", "eu", "br", "de"];
    let mut total_pii = 0;
    let total_pages = pages.len();

    // Layer 3 (QVAC LLM in the sidecar). Preparing it may start the sidecar
    // and load the model; any failure means Layers 1-2 only, never a failed
    // scan. The reason is surfaced to the UI via `layer3-status`.
    #[cfg(desktop)]
    let layer3_enabled = state.qvac.settings().layer3_enabled;
    #[cfg(not(desktop))]
    let layer3_enabled = false;
    let mut layer3_skip: Option<String> = None;
    let layer3_started = std::time::Instant::now();
    #[cfg(desktop)]
    let mut layer3 = if layer3_enabled {
        let _ = app_handle.emit("scan-progress", serde_json::json!({
            "documentId": id, "page": 0, "totalPages": total_pages,
            "llmActive": true, "phase": "llm-loading",
        }));
        match state.qvac.prepare_layer3() {
            Ok(session) => {
                if let Some(d) = session.loaded_in {
                    let _ = state.db.insert_audit_log(
                        Some(&id),
                        "model_load",
                        Some(&format!("model={} purpose=layer3 duration_ms={}", session.model, d.as_millis())),
                    );
                }
                let app_tokens = app_handle.clone();
                let doc_tokens = id.clone();
                session.set_token_callback(Some(std::sync::Arc::new(move |data: &serde_json::Value| {
                    let _ = app_tokens.emit("llm-progress", serde_json::json!({
                        "documentId": doc_tokens,
                        "tokensGenerated": data.get("tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                        "phase": "generating",
                    }));
                })));
                Some(session)
            }
            Err(reason) => {
                log::warn!("Layer 3 skipped for this scan: {}", reason);
                layer3_skip = Some(reason);
                None
            }
        }
    } else {
        None
    };
    #[cfg(desktop)]
    let layer3_model = layer3.as_ref().map(|s| s.model.clone());
    #[cfg(not(desktop))]
    let layer3_model: Option<String> = None;
    let mut layer3_entities = 0usize;
    let mut layer3_failed_chunks = 0usize;

    // Store text and run PII pipeline per page
    let text_dir = state.storage_dir.join("text");
    std::fs::create_dir_all(&text_dir)
        .map_err(|e| format!("Failed to create text dir: {}", e))?;

    for (page_num, text) in pages.iter().enumerate() {
        #[cfg(desktop)]
        let llm_active = layer3.is_some();
        #[cfg(not(desktop))]
        let llm_active = false;
        // Emit scan progress event
        let _ = app_handle.emit("scan-progress", serde_json::json!({
            "documentId": id,
            "page": page_num,
            "totalPages": total_pages,
            "llmActive": llm_active,
            "phase": if llm_active { "llm" } else { "heuristic" },
        }));
        // Save text to filesystem (backward compat)
        let text_path = text_dir.join(format!("{}_{}.txt", id, page_num));
        std::fs::write(&text_path, text)
            .map_err(|e| format!("Failed to write text: {}", e))?;

        // Store raw page text in database for reconstruction
        state
            .db
            .insert_document_page(&id, page_num as i32, text)
            .map_err(|e| format!("Failed to store page text: {}", e))?;

        // Run PII pipeline (Layer 3 only when prepared above)
        let app_handle_clone = app_handle.clone();
        let doc_id_clone = id.clone();
        let page_idx = page_num;
        let progress_cb: crate::llm::llm_scanner::ProgressFn = Box::new(move |tokens, phase| {
            let _ = app_handle_clone.emit("llm-progress", serde_json::json!({
                "documentId": doc_id_clone,
                "page": page_idx,
                "tokensGenerated": tokens,
                "phase": phase,
            }));
        });
        #[cfg(desktop)]
        let engine: Option<&dyn crate::llm::llm_scanner::Layer3Engine> =
            layer3.as_ref().map(|s| s as &dyn crate::llm::llm_scanner::Layer3Engine);
        #[cfg(not(desktop))]
        let engine: Option<&dyn crate::llm::llm_scanner::Layer3Engine> = None;
        let on_progress = if engine.is_some() { Some(&progress_cb) } else { None };
        let (entities, l3) = pipeline::run_pipeline(text, page_num as u32, &jurisdictions, engine, on_progress);
        match l3 {
            pipeline::Layer3Status::Ran { entities: n, failed_chunks, .. } => {
                layer3_entities += n;
                layer3_failed_chunks += failed_chunks;
            }
            pipeline::Layer3Status::Aborted(reason) => {
                layer3_entities += entities.iter().filter(|e| e.layer == crate::llm::llm_scanner::LAYER).count();
                log::warn!("Layer 3 stopped on page {}: {}", page_num, reason);
                layer3_skip = Some(reason);
                // Remaining pages: Layers 1-2 only.
                #[cfg(desktop)]
                {
                    layer3 = None;
                }
            }
            pipeline::Layer3Status::NotRun => {}
        }

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

    if layer3_enabled {
        let status = match (&layer3_model, &layer3_skip) {
            (None, Some(_)) => "skipped",
            (Some(_), Some(_)) => "partial",
            (Some(_), None) if layer3_failed_chunks > 0 => "partial",
            _ => "ran",
        };
        let _ = state.db.insert_audit_log(
            Some(&id),
            "ai_scan",
            Some(&format!(
                "layer=3 model={} status={} duration_ms={} entities={} pages={} failed_chunks={}{}",
                layer3_model.as_deref().unwrap_or("-"),
                status,
                layer3_started.elapsed().as_millis(),
                layer3_entities,
                pages.len(),
                layer3_failed_chunks,
                layer3_skip.as_ref().map(|r| format!(" reason={}", r)).unwrap_or_default(),
            )),
        );
        let _ = app_handle.emit("layer3-status", serde_json::json!({
            "documentId": id,
            "status": status,
            "reason": layer3_skip.clone().or_else(|| (layer3_failed_chunks > 0)
                .then(|| format!("{} text chunk(s) could not be analysed", layer3_failed_chunks))),
            "model": layer3_model,
            "entities": layer3_entities,
            "failedChunks": layer3_failed_chunks,
        }));
    }

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

/// Extract text from a DOCX file stored in encrypted storage
#[tauri::command]
pub async fn extract_docx_text(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let doc = state
        .db
        .get_document(&id)
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "Document not found".to_string())?;

    if doc.file_type != "docx" {
        return Err("Not a DOCX document".to_string());
    }

    // Decrypt and write to a temp file for the parser
    let decrypted = encryption::decrypt_file(Path::new(&doc.original_path))?;
    let tmp_path = state.storage_dir.join(format!("{}_tmp.docx", id));
    std::fs::write(&tmp_path, &decrypted)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;

    let pages = crate::document::docx_parser::extract_docx_text(&tmp_path)
        .map_err(|e| format!("Failed to extract DOCX text: {}", e))?;

    // Clean up temp file
    let _ = std::fs::remove_file(&tmp_path);

    Ok(pages.into_iter().map(|p| p.text).collect())
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

/// Load a document's decrypted bytes, its redactions and the path its clean
/// export should be written to.
fn prepare_export(
    id: &str,
    expected_type: &str,
    state: &AppState,
) -> Result<(Vec<u8>, Vec<crate::storage::database::RedactionRow>, std::path::PathBuf), String> {
    let doc = state
        .db
        .get_document(id)
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "Document not found".to_string())?;
    if doc.file_type != expected_type {
        return Err(format!("Not a {} document", expected_type.to_uppercase()));
    }
    let file_bytes = encryption::decrypt_file(Path::new(&doc.original_path))?;
    let redactions = state
        .db
        .get_redactions(id)
        .map_err(|e| format!("Database error: {}", e))?;

    let clean_dir = state.storage_dir.join("clean");
    std::fs::create_dir_all(&clean_dir)
        .map_err(|e| format!("Failed to create clean dir: {}", e))?;
    let base_name = doc.filename.rsplit_once('.').map(|(n, _)| n).unwrap_or(&doc.filename);
    let path = clean_dir.join(format!("{}_redacted.{}", base_name, expected_type));
    Ok((file_bytes, redactions, path))
}

fn record_export(id: &str, path: &Path, detail: &str, state: &AppState) -> Result<String, String> {
    state
        .db
        .update_document_clean_path(id, path.to_str().unwrap_or(""))
        .map_err(|e| format!("Database error: {}", e))?;
    let _ = state.db.insert_audit_log(Some(id), "export", Some(detail));
    Ok(path.to_string_lossy().to_string())
}

/// Export a redacted DOCX (run-aware rewrite of every text-bearing part,
/// verified before returning).
// TODO: On mobile, return path for share sheet integration
#[tauri::command]
pub async fn export_redacted_docx(
    id: String,
    redaction_mode: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mode = match redaction_mode.as_deref() {
        Some("black_bar") => crate::document::redactor::RedactionMode::BlackBar,
        _ => crate::document::redactor::RedactionMode::TextReplace,
    };
    let (file_bytes, redactions, path) = prepare_export(&id, "docx", &state)?;
    crate::document::redactor::export_redacted_docx(&file_bytes, &redactions, &path, mode)?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    record_export(&id, &path, &format!("Exported redacted DOCX: {}", name), &state)
}

/// One page to rasterize, as rendered by PDF.js, with the boxes to burn in.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactedPageInput {
    /// 0-based page index.
    pub page: u32,
    /// PNG or JPEG, base64.
    pub image_base64: String,
    pub rects: Vec<crate::document::redactor::PixelRect>,
}

/// Export a redacted PDF: pages with approved redactions are rasterized with
/// the boxes burned in by Rust; other pages keep their content. Refuses if a
/// redaction cannot be placed or approved text would survive (in the text
/// layer, or — for scanned pages — in OCR of the burned images). Pages that
/// need OCR but were not scanned require `acknowledge_unscanned`.
// TODO: On mobile, return path for share sheet integration
#[tauri::command]
pub async fn export_redacted_pdf(
    id: String,
    pages: Vec<RedactedPageInput>,
    acknowledge_unscanned: Option<bool>,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    use base64::Engine;
    let (file_bytes, redactions, path) = prepare_export(&id, "pdf", &state)?;

    // ── Scanned pages: stored OCR layouts, and pages that were never scanned ──
    let ocr_rows = state.db.get_ocr_pages(&id).map_err(|e| format!("Database error: {}", e))?;
    let mut layouts = std::collections::BTreeMap::new();
    let mut unscanned = std::collections::BTreeSet::new();
    for row in &ocr_rows {
        let page = row.page.max(0) as u32;
        match (row.status.as_str(), row.layout.as_deref()) {
            ("done", Some(json)) => {
                let layout: crate::ocr::layout::PageLayout =
                    serde_json::from_str(json).map_err(|_| format!("Export failed: bad OCR data for page {}.", page + 1))?;
                layouts.insert(page, layout);
            }
            _ => {
                unscanned.insert(page);
            }
        }
    }
    // Do not rely on the viewer having reported every page: re-classify here.
    {
        let mut pdf = lopdf::Document::load_mem(&file_bytes).map_err(|e| format!("Failed to load PDF: {}", e))?;
        if pdf.is_encrypted() {
            let _ = pdf.decrypt("");
        }
        for c in crate::ocr::classify::classify_pdf(&pdf, &[]) {
            if c.needs_ocr && !layouts.contains_key(&c.page) {
                unscanned.insert(c.page);
            }
        }
    }
    let acknowledged = acknowledge_unscanned.unwrap_or(false);
    let unscanned_list = unscanned.iter().map(|p| (p + 1).to_string()).collect::<Vec<_>>().join(", ");
    if !unscanned.is_empty() && !acknowledged {
        // Machine-readable prefix for the viewer, then the message.
        return Err(format!(
            "OCR_UNSCANNED:{}|Page(s) {} are images that were not scanned for personal data (OCR was not \
             available or was cancelled). Anything on them will be exported as is.",
            unscanned.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(","),
            unscanned_list
        ));
    }

    let rasters = pages
        .into_iter()
        .map(|p| {
            let image = base64::engine::general_purpose::STANDARD
                .decode(&p.image_base64)
                .map_err(|e| format!("Failed to decode page image: {}", e))?;
            Ok(crate::document::redactor::PageRaster { page_index: p.page, image, rects: p.rects })
        })
        .collect::<Result<Vec<_>, String>>()?;

    #[cfg(desktop)]
    let engine_svc = std::sync::Arc::clone(&state.qvac);
    #[cfg(desktop)]
    let work_dir = engine_svc.page_work_dir()?;
    #[cfg(not(desktop))]
    let work_dir = state.storage_dir.join("ocr-tmp");
    let out_path = path.clone();
    let progress_app = app_handle.clone();
    let progress_id = id.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || {
        let progress = move |done: usize, total: usize| {
            let _ = progress_app.emit(
                "export-progress",
                serde_json::json!({"documentId": progress_id, "phase": "ocr-verify", "done": done, "total": total}),
            );
        };
        #[cfg(desktop)]
        let engine: Option<&dyn crate::ocr::PageOcr> =
            (engine_svc.is_bundled() && engine_svc.ocr_installed()).then_some(&*engine_svc as &dyn crate::ocr::PageOcr);
        #[cfg(not(desktop))]
        let engine: Option<&dyn crate::ocr::PageOcr> = None;
        let ocr = crate::document::redactor::OcrExport { layouts, engine, work_dir, on_progress: Some(&progress) };
        crate::document::redactor::export_flattened_pdf_ocr(&file_bytes, &redactions, &rasters, &out_path, Some(&ocr))
    })
    .await
    .map_err(|e| format!("Export failed: {}", e))??;

    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut detail = format!(
        "Exported redacted PDF: {} ({} of {} pages rasterized",
        name,
        summary.rasterized_pages.len(),
        summary.page_count
    );
    if !summary.ocr_verified_pages.is_empty() {
        detail.push_str(&format!(", {} scanned page(s) verified with OCR", summary.ocr_verified_pages.len()));
    }
    if !unscanned.is_empty() {
        detail.push_str(&format!(", user acknowledged unscanned page(s) {}", unscanned_list));
    }
    detail.push(')');
    record_export(&id, &path, &detail, &state)
}

#[tauri::command]
pub async fn open_file(path: String, app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("Failed to open file: {}", e))
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
            DELETE FROM ocr_pages;
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
