use tauri::State;
use crate::storage::database::CreditRow;
use crate::AppState;

#[tauri::command]
pub async fn get_credits(
    state: State<'_, AppState>,
) -> Result<CreditRow, String> {
    state
        .db
        .get_or_init_credits()
        .map_err(|e| format!("Database error: {}", e))
}
