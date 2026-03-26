use serde::Serialize;
use tauri::State;
use crate::AppState;
use crate::llm::model_manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmStatus {
    /// Whether the model file exists on disk.
    pub available: bool,
    /// Whether the LLM layer is enabled for scanning.
    pub enabled: bool,
    /// Whether a download is currently in progress.
    pub downloading: bool,
    /// Download progress 0.0 to 1.0 (only meaningful when downloading).
    pub download_progress: f64,
    /// Whether the model is loaded into memory and ready for inference.
    pub loaded: bool,
}

#[tauri::command]
pub async fn llm_status(
    state: State<'_, AppState>,
) -> Result<LlmStatus, String> {
    let llm = &state.llm_state;
    let progress = *llm.download_progress.lock().unwrap();

    Ok(LlmStatus {
        available: model_manager::is_model_available(&state.storage_dir),
        enabled: llm.is_enabled(),
        downloading: llm.is_downloading(),
        download_progress: progress,
        #[cfg(feature = "llm")]
        loaded: llm.is_model_loaded(),
        #[cfg(not(feature = "llm"))]
        loaded: false,
    })
}

/// Start downloading the model in a background thread.
/// Returns immediately; poll `llm_status` for progress.
#[tauri::command]
pub async fn llm_download_model(
    state: State<'_, AppState>,
) -> Result<(), String> {
    if state.llm_state.is_downloading() {
        return Err("Download already in progress".to_string());
    }

    #[cfg(feature = "llm")]
    {
        let storage_dir = state.storage_dir.clone();
        let llm_state = state.llm_state.clone();

        // Spawn blocking download in a background thread
        tokio::task::spawn_blocking(move || {
            if let Err(e) = model_manager::download_model(&storage_dir, &llm_state) {
                log::error!("Model download failed: {}", e);
            }
        });

        Ok(())
    }

    #[cfg(not(feature = "llm"))]
    Err("LLM feature not enabled in this build".to_string())
}

/// Load the downloaded model into memory for inference.
#[tauri::command]
pub async fn llm_load_model(
    #[allow(unused_variables)] state: State<'_, AppState>,
) -> Result<(), String> {
    #[cfg(feature = "llm")]
    {
        let storage_dir = state.storage_dir.clone();
        let llm_state = state.llm_state.clone();

        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let runtime = model_manager::load_model(&storage_dir)?;
            *llm_state.runtime.lock().unwrap() = Some(runtime);
            llm_state.set_enabled(true);
            llm_state.touch();
            llm_state.start_idle_timer();
            Ok(())
        })
        .await
        .map_err(|e| format!("Task join failed: {}", e))?
    }

    #[cfg(not(feature = "llm"))]
    Err("LLM feature not enabled in this build".to_string())
}

/// Enable or disable the LLM detection layer.
#[tauri::command]
pub async fn llm_set_enabled(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.llm_state.set_enabled(enabled);
    Ok(())
}
