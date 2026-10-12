//! OCR commands for scanned PDF pages and imported images.
//!
//! Flow (driven by the viewer, see DocumentViewer.tsx):
//! 1. `classify_pdf_pages` — which pages need OCR (ocr::classify).
//! 2. `ocr_status` — can OCR run now (sidecar bundled, models installed)?
//! 3. per page: the frontend renders the page with PDF.js at the OCR DPI and
//!    calls `ocr_page`; the image goes to an app-private temp file, through
//!    the sidecar, and is deleted. The layout (normalised text + boxes) is
//!    stored and returned. `ocr_cancel` stops the run.
//! 4. `ocr_finish` — records pages that could not be scanned (unavailable,
//!    cancelled, failed) and writes one `ocr_run` audit line (metadata only).
//!
//! Export reads the stored layouts and refuses — unless the user explicitly
//! acknowledges — when a page that needs OCR was not scanned.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::ocr::classify::{classify_pdf, PageClass};
use crate::ocr::layout::PageLayout;
use crate::storage::database::OcrPageRow;
use crate::storage::encryption;
use crate::AppState;

fn load_pdf(id: &str, state: &AppState) -> Result<lopdf::Document, String> {
    let doc = state
        .db
        .get_document(id)
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "Document not found".to_string())?;
    if doc.file_type != "pdf" {
        return Err("Not a PDF document".into());
    }
    let bytes = encryption::decrypt_file(Path::new(&doc.original_path))?;
    let mut pdf = lopdf::Document::load_mem(&bytes).map_err(|e| format!("Failed to load PDF: {}", e))?;
    if pdf.is_encrypted() {
        let _ = pdf.decrypt("");
    }
    Ok(pdf)
}

/// Which pages need OCR. `text_chars[i]` is the number of visible characters
/// PDF.js found on page i (the smaller of PDF.js's and the backend's count is
/// used).
#[tauri::command]
pub async fn classify_pdf_pages(
    id: String,
    text_chars: Vec<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<PageClass>, String> {
    let pdf = load_pdf(&id, &state)?;
    let counts: Vec<usize> = text_chars.iter().map(|&c| c as usize).collect();
    Ok(classify_pdf(&pdf, &counts))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrStatus {
    pub available: bool,
    /// ok | unsupported (mobile) | not_bundled | models_missing | engine_unavailable
    pub code: String,
    pub reason: Option<String>,
    pub dpi: u32,
}

/// Whether OCR can run now. Cheap: does not start the sidecar.
#[tauri::command]
pub async fn ocr_status(state: State<'_, AppState>) -> Result<OcrStatus, String> {
    #[cfg(desktop)]
    {
        let caps = state.qvac.capabilities(false);
        let (code, reason) = if !caps.sidecar_bundled {
            ("not_bundled", caps.unavailable_reason.clone())
        } else if !caps.ocr_installed {
            ("models_missing", Some("OCR models are not installed".to_string()))
        } else if !caps.sidecar_available {
            ("engine_unavailable", caps.unavailable_reason.clone())
        } else {
            ("ok", None)
        };
        Ok(OcrStatus { available: code == "ok", code: code.into(), reason, dpi: caps.ocr_dpi })
    }
    #[cfg(not(desktop))]
    {
        let _ = state;
        Ok(OcrStatus {
            available: false,
            code: "unsupported".into(),
            reason: Some("OCR is not available on this platform".into()),
            dpi: crate::ocr::DEFAULT_OCR_DPI,
        })
    }
}

/// OCR one page image (PNG or JPEG, base64) rendered at `dpi`. Errors start
/// with `OCR_CANCELLED`, `OCR_UNAVAILABLE:` or `OCR_FAILED:` so the viewer
/// can tell them apart.
#[cfg(desktop)]
#[tauri::command]
pub async fn ocr_page(
    id: String,
    page: u32,
    image_base64: String,
    dpi: u32,
    state: State<'_, AppState>,
) -> Result<PageLayout, String> {
    use base64::Engine;
    use crate::ocr::OcrFailure;

    let pdf = load_pdf(&id, &state)?;
    let page_ids: Vec<lopdf::ObjectId> = pdf.get_pages().values().copied().collect();
    let page_id = *page_ids.get(page as usize).ok_or("OCR_FAILED: page out of range")?;
    let (w_pt, h_pt) = crate::document::pdf_redact::displayed_size(&pdf, page_id);
    drop(pdf);

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image_base64)
        .map_err(|e| format!("OCR_FAILED: bad page image: {}", e))?;
    drop(image_base64);
    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| format!("OCR_FAILED: bad page image: {}", e))?;
    let ext = match reader.format() {
        Some(image::ImageFormat::Png) => "png",
        Some(image::ImageFormat::Jpeg) => "jpg",
        _ => return Err("OCR_FAILED: page image must be PNG or JPEG".into()),
    };
    let (img_w, img_h) = reader.into_dimensions().map_err(|e| format!("OCR_FAILED: bad page image: {}", e))?;
    if img_w == 0 || img_h == 0 || ((img_w as f64 / img_h as f64) / (w_pt / h_pt) - 1.0).abs() > 0.03 {
        return Err("OCR_FAILED: page image does not match the page shape".into());
    }

    let svc = std::sync::Arc::clone(&state.qvac);
    let run = state.ocr_runs.begin(&id, dpi);
    let cancel: crate::qvac::client::CancelToken = run.cancel.clone().into();
    if cancel.is_cancelled() {
        return Err("OCR_CANCELLED".into());
    }
    let started = std::time::Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let dir = svc.page_work_dir().map_err(OcrFailure::Failed)?;
        let path = dir.join(format!("{}.{}", uuid::Uuid::new_v4(), ext));
        let write = {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            opts.open(&path).and_then(|mut f| f.write_all(&bytes))
        };
        drop(bytes);
        let r = match write {
            Ok(()) => svc.ocr_blocks(&path, Some(cancel)),
            Err(e) => Err(OcrFailure::Failed(format!("cannot write page image: {}", e.kind()))),
        };
        let _ = std::fs::remove_file(&path);
        r
    })
    .await
    .map_err(|e| format!("OCR_FAILED: task failed: {}", e))?;
    let ms = started.elapsed().as_millis();

    let blocks = match result {
        Ok(b) => b,
        Err(OcrFailure::Cancelled) => return Err("OCR_CANCELLED".into()),
        Err(OcrFailure::Unavailable(r)) => return Err(format!("OCR_UNAVAILABLE: {}", r)),
        Err(OcrFailure::Failed(r)) => return Err(format!("OCR_FAILED: {}", r)),
    };
    let layout = crate::ocr::layout::build_layout(&blocks, img_w, img_h, w_pt, h_pt, dpi);
    let json = serde_json::to_string(&layout).map_err(|e| format!("OCR_FAILED: {}", e))?;
    state
        .db
        .upsert_ocr_page(&id, page as i32, "done", None, Some(dpi as i64), Some(&json), Some(blocks.len() as i64), Some(ms as i64))
        .map_err(|e| format!("OCR_FAILED: database error: {}", e))?;
    state.ocr_runs.record_page(&id, page, blocks.len(), ms);
    Ok(layout)
}

/// Cancel the OCR run of a document (the in-flight sidecar request gets a
/// `cancel`).
#[cfg(desktop)]
#[tauri::command]
pub async fn ocr_cancel(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.ocr_runs.cancel(&id))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnscannedPage {
    pub page: u32,
    /// unavailable | cancelled | failed
    pub status: String,
    pub reason: Option<String>,
}

/// End an OCR pass: record pages that need OCR but were not scanned, and
/// audit-log the run (document id, pages, DPI, duration, block counts).
#[tauri::command]
pub async fn ocr_finish(
    id: String,
    unscanned: Vec<UnscannedPage>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    for u in &unscanned {
        let status = match u.status.as_str() {
            "unavailable" | "cancelled" | "failed" => u.status.as_str(),
            _ => "failed",
        };
        // The reason is an engine/status message, never document text.
        let reason: Option<String> = u.reason.as_ref().map(|r| r.chars().take(200).collect());
        state
            .db
            .upsert_ocr_page(&id, u.page as i32, status, reason.as_deref(), None, None, None, None)
            .map_err(|e| format!("Database error: {}", e))?;
    }
    let run = state.ocr_runs.finish(&id);
    let list = |v: &[u32]| v.iter().map(|p| (p + 1).to_string()).collect::<Vec<_>>().join(",");
    let skipped: Vec<u32> = unscanned.iter().map(|u| u.page).collect();
    let detail = match &run {
        Some(r) => {
            let status = if skipped.is_empty() {
                "ran"
            } else if unscanned.iter().any(|u| u.status == "cancelled") {
                "cancelled"
            } else {
                "partial"
            };
            format!(
                "status={} pages={} dpi={} duration_ms={} ocr_ms={} blocks={}{}",
                status,
                list(&r.pages_done),
                r.dpi,
                r.started.elapsed().as_millis(),
                r.ocr_ms,
                r.blocks,
                if skipped.is_empty() { String::new() } else { format!(" unscanned={}", list(&skipped)) }
            )
        }
        None if skipped.is_empty() => return Ok(()),
        None => format!(
            "status=skipped unscanned={} reason={}",
            list(&skipped),
            unscanned.first().map(|u| u.status.as_str()).unwrap_or("unavailable")
        ),
    };
    let _ = state.db.insert_audit_log(Some(&id), "ocr_run", Some(&detail));
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrPageInfo {
    pub page: u32,
    pub status: String,
    pub reason: Option<String>,
    pub dpi: Option<u32>,
    pub layout: Option<PageLayout>,
}

impl From<OcrPageRow> for OcrPageInfo {
    fn from(r: OcrPageRow) -> Self {
        OcrPageInfo {
            page: r.page.max(0) as u32,
            layout: r.layout.as_deref().and_then(|j| serde_json::from_str(j).ok()),
            status: r.status,
            reason: r.reason,
            dpi: r.dpi.map(|d| d as u32),
        }
    }
}

/// Stored OCR state of a document's pages (layouts of scanned pages are
/// reused when the document is opened again at the same DPI).
#[tauri::command]
pub async fn get_ocr_pages(id: String, state: State<'_, AppState>) -> Result<Vec<OcrPageInfo>, String> {
    let rows = state.db.get_ocr_pages(&id).map_err(|e| format!("Database error: {}", e))?;
    Ok(rows.into_iter().map(OcrPageInfo::from).collect())
}

/// Set the OCR rasterisation resolution (Settings).
#[cfg(desktop)]
#[tauri::command]
pub async fn ai_set_ocr_dpi(dpi: u32, state: State<'_, AppState>) -> Result<(), String> {
    state.qvac.set_ocr_dpi(dpi)?;
    let _ = state.db.insert_audit_log(None, "ai_settings", Some(&format!("ocr_dpi={}", dpi)));
    Ok(())
}
