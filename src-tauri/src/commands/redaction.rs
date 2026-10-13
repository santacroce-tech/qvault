use tauri::State;
use crate::storage::database::RedactionRow;
use crate::AppState;

#[tauri::command]
pub async fn get_redactions(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<RedactionRow>, String> {
    state
        .db
        .get_redactions(&document_id)
        .map_err(|e| format!("Database error: {}", e))
}

#[tauri::command]
pub async fn update_redaction(
    id: String,
    status: String,
    replacement: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .update_redaction_status(&id, &status, replacement.as_deref())
        .map_err(|e| format!("Database error: {}", e))?;
    // Decision memory (§13): metadata only, never logged.
    if let Err(e) = crate::learning::service::record_decision(&state.db, &id, &status) {
        log::warn!("review decision not remembered: {}", e);
    }
    Ok(())
}

#[tauri::command]
pub async fn bulk_approve(
    document_id: String,
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .bulk_approve_redactions(&document_id, category.as_deref())
        .map_err(|e| format!("Database error: {}", e))?;
    if let Err(e) = crate::learning::service::record_document_decisions(&state.db, &document_id) {
        log::warn!("review decisions not remembered: {}", e);
    }
    Ok(())
}
