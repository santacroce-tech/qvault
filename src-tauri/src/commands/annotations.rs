use tauri::State;
use uuid::Uuid;
use crate::storage::database::AnnotationRow;
use crate::AppState;

#[tauri::command]
pub async fn add_annotation(
    document_id: String,
    page: i32,
    kind: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    content: String,
    font_size: Option<f64>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();
    state
        .db
        .insert_annotation(&id, &document_id, page, &kind, x, y, width, height, &content, font_size)
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(id)
}

#[tauri::command]
pub async fn update_annotation(
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    content: String,
    font_size: Option<f64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .update_annotation(&id, x, y, width, height, &content, font_size)
        .map_err(|e| format!("Database error: {}", e))
}

#[tauri::command]
pub async fn delete_annotation(
    id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .delete_annotation(&id)
        .map_err(|e| format!("Database error: {}", e))
}

#[tauri::command]
pub async fn get_annotations(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AnnotationRow>, String> {
    state
        .db
        .get_annotations(&document_id)
        .map_err(|e| format!("Database error: {}", e))
}
