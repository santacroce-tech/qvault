//! Learning from review decisions (§13): the review sidebar's view of a
//! document (marks and suppressions), the one-click "reject all previously
//! rejected", restoring a suppressed finding, and the "Not personal data"
//! list (add, remove, export, import with preview, suggested candidates).
//! The audit log gets counts and ids only, never the strings.

use tauri::State;

use crate::learning::service::{self, Candidate, DocumentLearning, LearningSummary, ListItem};
use crate::learning::{ImportPreview, ListFileItem};
use crate::AppState;

#[tauri::command]
pub async fn get_document_learning(document_id: String, state: State<'_, AppState>) -> Result<DocumentLearning, String> {
    service::document_learning(&state.db, &document_id)
}

#[tauri::command]
pub async fn reject_previously_rejected(document_id: String, state: State<'_, AppState>) -> Result<usize, String> {
    let n = service::reject_previously_rejected(&state.db, &document_id)?;
    let _ = state.db.insert_audit_log(Some(&document_id), "reject_previously_rejected", Some(&format!("count={}", n)));
    Ok(n)
}

#[tauri::command]
pub async fn restore_suppressed_finding(id: String, document_id: String, state: State<'_, AppState>) -> Result<String, String> {
    let rid = service::restore_suppressed(&state.db, &id)?;
    let _ = state.db.insert_audit_log(Some(&document_id), "suppressed_restored", Some(&format!("redaction={}", rid)));
    Ok(rid)
}

#[tauri::command]
pub async fn get_not_personal_data(state: State<'_, AppState>) -> Result<Vec<ListItem>, String> {
    service::list_items(&state.db)
}

#[tauri::command]
pub async fn add_not_personal_data(
    category: String,
    text: String,
    note: Option<String>,
    source: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let id = service::add_item(&state.db, &category, &text, note.as_deref(), &source)?;
    let _ = state.db.insert_audit_log(None, "not_personal_data_add", Some(&format!("category={} source={} item={}", category, source, id)));
    Ok(id)
}

#[tauri::command]
pub async fn remove_not_personal_data(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let removed = service::remove_item(&state.db, &id)?;
    if removed {
        let _ = state.db.insert_audit_log(None, "not_personal_data_remove", Some(&format!("item={}", id)));
    }
    Ok(removed)
}

/// Write the list to `path` (chosen by the user in a save dialog).
#[tauri::command]
pub async fn export_not_personal_data(path: String, state: State<'_, AppState>) -> Result<usize, String> {
    let json = service::export_json(&state.db)?;
    let n = service::list_items(&state.db)?.len();
    std::fs::write(&path, json).map_err(|e| format!("Could not write the file: {}", e))?;
    let _ = state.db.insert_audit_log(None, "not_personal_data_export", Some(&format!("items={}", n)));
    Ok(n)
}

/// Largest list file accepted (10,000 items of 200 characters fit easily).
const IMPORT_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Validate a list file and show what would be added; changes nothing.
#[tauri::command]
pub async fn preview_not_personal_data_import(path: String, state: State<'_, AppState>) -> Result<ImportPreview, String> {
    let meta = std::fs::metadata(&path).map_err(|e| format!("Could not read the file: {}", e))?;
    if meta.len() > IMPORT_MAX_BYTES {
        return Err("The file is too large to be a Qvault list.".into());
    }
    let json = std::fs::read_to_string(&path).map_err(|e| format!("Could not read the file: {}", e))?;
    service::preview_import(&state.db, &json)
}

/// Add the items the user confirmed after the preview.
#[tauri::command]
pub async fn apply_not_personal_data_import(items: Vec<ListFileItem>, state: State<'_, AppState>) -> Result<usize, String> {
    let n = service::apply_import(&state.db, &items)?;
    let _ = state.db.insert_audit_log(None, "not_personal_data_import", Some(&format!("offered={} added={}", items.len(), n)));
    Ok(n)
}

#[tauri::command]
pub async fn get_not_personal_data_candidates(state: State<'_, AppState>) -> Result<Vec<Candidate>, String> {
    service::candidates(&state.db)
}

#[tauri::command]
pub async fn get_learning_summary(state: State<'_, AppState>) -> Result<LearningSummary, String> {
    service::summary(&state.db)
}

/// Forget all remembered review decisions (the list is kept).
#[tauri::command]
pub async fn clear_review_memory(state: State<'_, AppState>) -> Result<usize, String> {
    let n = service::clear_decisions(&state.db)?;
    let _ = state.db.insert_audit_log(None, "review_memory_cleared", Some(&format!("decisions={}", n)));
    Ok(n)
}
