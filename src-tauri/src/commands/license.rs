use tauri::State;
use crate::storage::database::LicenseRow;
use crate::AppState;

#[tauri::command]
pub async fn get_license(
    state: State<'_, AppState>,
) -> Result<Option<LicenseRow>, String> {
    state
        .db
        .get_license()
        .map_err(|e| format!("Database error: {}", e))
}

#[tauri::command]
pub async fn activate_license(
    license_key: String,
    state: State<'_, AppState>,
) -> Result<LicenseRow, String> {
    // Validate license key format: QVLT-XXXX-XXXX-XXXX
    let key = license_key.trim().to_uppercase();
    if !is_valid_license_format(&key) {
        return Err("Invalid license key format. Expected: QVLT-XXXX-XXXX-XXXX".to_string());
    }

    // Calculate expiry: 1 year from now
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::days(365);
    let expires_at = expires.to_rfc3339();

    state
        .db
        .activate_license(&key, &expires_at)
        .map_err(|e| format!("Failed to activate license: {}", e))?;

    // Audit log
    let _ = state.db.insert_audit_log(
        None,
        "license_activated",
        Some(&format!("License key: {}...{}", &key[..8], &key[key.len()-4..])),
    );

    state
        .db
        .get_license()
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "License activation failed".to_string())
}

#[tauri::command]
pub async fn deactivate_license(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .deactivate_license()
        .map_err(|e| format!("Failed to deactivate license: {}", e))?;

    let _ = state.db.insert_audit_log(None, "license_deactivated", None);

    Ok(())
}

#[tauri::command]
pub async fn check_license(
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state
        .db
        .is_licensed()
        .map_err(|e| format!("License check failed: {}", e))
}

fn is_valid_license_format(key: &str) -> bool {
    let parts: Vec<&str> = key.split('-').collect();
    if parts.len() != 4 {
        return false;
    }
    if parts[0] != "QVLT" {
        return false;
    }
    // Each part after prefix should be 4 alphanumeric chars
    parts[1..].iter().all(|p| p.len() == 4 && p.chars().all(|c| c.is_alphanumeric()))
}
