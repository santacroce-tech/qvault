//! AI (QVAC) commands: capability probe, Model Manager, Layer 3 settings and
//! a self-test. Desktop only. Every state change is audit-logged with
//! metadata only (model ids, sizes, durations — never document content).

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{Emitter, Manager, State};

use crate::qvac::models::ImportOutcome;
use crate::qvac::{Capabilities, FitInfo, ModelInfo, QvacService, SelfTestReport};
use crate::AppState;

fn svc(state: &State<'_, AppState>) -> Arc<QvacService> {
    Arc::clone(&state.qvac)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| format!("task failed: {}", e))
}

/// Capability probe. With `probe = true` the sidecar is started (if bundled)
/// and fit verdicts are computed for installed LLMs.
#[tauri::command]
pub async fn ai_capabilities(probe: Option<bool>, state: State<'_, AppState>) -> Result<Capabilities, String> {
    let s = svc(&state);
    blocking(move || s.capabilities(probe.unwrap_or(false))).await
}

#[tauri::command]
pub async fn ai_list_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let s = svc(&state);
    blocking(move || s.list_models()).await
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress<'a> {
    id: &'a str,
    received: u64,
    total: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadDone<'a> {
    id: &'a str,
    ok: bool,
    error: Option<String>,
}

/// Start a user-initiated HTTPS download. Progress: `ai-model-download`
/// events; completion: `ai-model-download-done`.
#[tauri::command]
pub async fn ai_download_model(id: String, state: State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    let s = svc(&state);
    let progress_app = app.clone();
    let done_app = app.clone();
    let size = crate::qvac::catalog::find(&id).map(|m| m.size).unwrap_or(0);
    let _ = state.db.insert_audit_log(None, "model_download_start", Some(&format!("model={} size={}", id, size)));
    s.start_download(
        &id,
        move |id, received, total| {
            let _ = progress_app.emit("ai-model-download", DownloadProgress { id, received, total });
        },
        move |id, result| {
            let st = done_app.state::<AppState>();
            let (action, details, ok, error) = match &result {
                Ok(d) => ("model_download", format!("model={} size={} sha256=verified duration_ms={}", id, size, d.as_millis()), true, None),
                Err(e) if e == "cancelled" => ("model_download_cancelled", format!("model={}", id), false, None),
                Err(e) => ("model_download_failed", format!("model={} reason={}", id, e), false, Some(e.clone())),
            };
            let _ = st.db.insert_audit_log(None, action, Some(&details));
            let _ = done_app.emit("ai-model-download-done", DownloadDone { id, ok, error });
        },
    )
}

#[tauri::command]
pub async fn ai_cancel_download(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.qvac.cancel_download(&id))
}

/// Import a model from a local file. Unknown files return `needsConsent`
/// until called again with `acceptUnknown = true`.
#[tauri::command]
pub async fn ai_import_model(
    path: String,
    accept_unknown: Option<bool>,
    state: State<'_, AppState>,
) -> Result<ImportOutcome, String> {
    let s = svc(&state);
    let accept = accept_unknown.unwrap_or(false);
    let p = PathBuf::from(&path);
    let started = std::time::Instant::now();
    let r = blocking(move || s.import_model(&p, accept)).await??;
    let details = match &r {
        ImportOutcome::Installed { id } => Some(format!("model={} sha256=verified duration_ms={}", id, started.elapsed().as_millis())),
        ImportOutcome::InstalledCustom { id } => Some(format!(
            "model={} custom=true user_accepted_unknown_checksum=true duration_ms={}",
            id,
            started.elapsed().as_millis()
        )),
        ImportOutcome::NeedsConsent { .. } => None,
    };
    if let Some(d) = details {
        let _ = state.db.insert_audit_log(None, "model_import", Some(&d));
    }
    Ok(r)
}

#[tauri::command]
pub async fn ai_delete_model(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let s = svc(&state);
    let id2 = id.clone();
    blocking(move || s.delete_model(&id2)).await??;
    let _ = state.db.insert_audit_log(None, "model_delete", Some(&format!("model={}", id)));
    Ok(())
}

#[tauri::command]
pub async fn ai_assess_fit(id: String, state: State<'_, AppState>) -> Result<FitInfo, String> {
    let s = svc(&state);
    blocking(move || s.assess_fit(&id)).await?
}

#[tauri::command]
pub async fn ai_set_layer3(enabled: bool, model_id: Option<String>, state: State<'_, AppState>) -> Result<(), String> {
    let s = svc(&state);
    let m = model_id.clone();
    blocking(move || s.set_layer3(enabled, m)).await??;
    let _ = state.db.insert_audit_log(
        None,
        "ai_settings",
        Some(&format!("layer3_enabled={} model={}", enabled, model_id.unwrap_or_else(|| "-".into()))),
    );
    Ok(())
}

/// Turn the embedding model's use for entity grouping on or off.
#[tauri::command]
pub async fn ai_set_entity_embeddings(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    let s = svc(&state);
    blocking(move || s.set_entity_embeddings(enabled)).await??;
    let _ = state.db.insert_audit_log(None, "ai_settings", Some(&format!("entity_embeddings={}", enabled)));
    Ok(())
}

/// Run the engine self-test (synthetic inputs only).
#[tauri::command]
pub async fn ai_self_test(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<SelfTestReport, String> {
    let s = svc(&state);
    let img = app
        .path()
        .resource_dir()
        .ok()
        .map(|d| d.join("selftest").join("ocr-sample.png"))
        .filter(|p| p.is_file());
    let rep = blocking(move || s.self_test(img.as_deref())).await?;
    let _ = state.db.insert_audit_log(
        None,
        "ai_self_test",
        Some(&format!(
            "ok={} ocr_ms={:?} layer3_model={:?} layer3_ms={:?} layer3_entities={} embedding_ms={:?}",
            rep.error.is_none() && rep.ocr_error.is_none() && rep.layer3_error.is_none() && rep.embedding_error.is_none(),
            rep.ocr_ms,
            rep.layer3_model,
            rep.layer3_ms,
            rep.layer3_entities.len(),
            rep.embedding_ms
        )),
    );
    Ok(rep)
}

/// Open the pinned manual-download link of an import-only catalog model in the
/// user's browser. Only catalog URLs can be opened; the webview never passes a
/// URL. The app itself does not download the file.
#[tauri::command]
pub async fn ai_open_manual_download(id: String, app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let url = crate::qvac::catalog::CATALOG
        .iter()
        .find(|m| m.id == id)
        .and_then(|m| m.manual_url)
        .ok_or_else(|| "This model has no manual download link".to_string())?;
    app.opener().open_url(url, None::<&str>).map_err(|e| format!("Could not open the browser: {}", e))
}
