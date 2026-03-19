use tauri::State;
use crate::storage::database::DocumentRow;
use crate::AppState;

#[tauri::command]
pub async fn get_history(
    offset: i64,
    limit: i64,
    state: State<'_, AppState>,
) -> Result<Vec<DocumentRow>, String> {
    state
        .db
        .get_history(offset, limit)
        .map_err(|e| format!("Database error: {}", e))
}

#[tauri::command]
pub async fn search_history(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<DocumentRow>, String> {
    state
        .db
        .search_history(&query)
        .map_err(|e| format!("Database error: {}", e))
}
