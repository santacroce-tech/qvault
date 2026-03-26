use serde::{Deserialize, Serialize};
use tauri::State;
use crate::storage::database::LicenseRow;
use crate::AppState;

/// License server URL — change this to your production server
#[cfg(debug_assertions)]
const LICENSE_SERVER_URL: &str = "http://localhost:3000";

#[cfg(not(debug_assertions))]
const LICENSE_SERVER_URL: &str = "https://api.qvault.tech";

/// Generate a stable machine ID from hostname + OS info
fn get_machine_id() -> String {
    // TODO: hostname dependency removed for mobile compatibility
    let host = "qvault-device".to_string();
    let raw = format!("qvault-{}-{}", host, std::env::consts::OS);
    let hash = ring::digest::digest(&ring::digest::SHA256, raw.as_bytes());
    hex::encode(&hash.as_ref()[..16]) // 32-char hex string
}

// ─── Server request/response types ───

#[derive(Serialize)]
struct LicenseRequest {
    license_key: String,
    machine_id: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ActivateResponse {
    valid: bool,
    license_key: Option<String>,
    expires_at: Option<String>,
    plan: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(debug_assertions, allow(dead_code))]
struct ValidateResponse {
    valid: bool,
    expires_at: Option<String>,
    error: Option<String>,
}

// ─── IPC Commands ───

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
    let key = license_key.trim().to_uppercase();
    if !is_valid_license_format(&key) {
        return Err("Invalid license key format. Expected: QVLT-XXXX-XXXX-XXXX".to_string());
    }

    let machine_id = get_machine_id();

    // Call license server for activation
    let server_result = call_activate_server(&key, &machine_id).await;

    match server_result {
        Ok(resp) if resp.valid => {
            let expires_at = resp.expires_at.unwrap_or_else(|| {
                let expires = chrono::Utc::now() + chrono::Duration::days(365);
                expires.to_rfc3339()
            });

            state
                .db
                .activate_license(&key, &machine_id, &expires_at)
                .map_err(|e| format!("Failed to store license: {}", e))?;

            let _ = state.db.insert_audit_log(
                None,
                "license_activated",
                Some(&format!("Key: {}...{}", &key[..8], &key[key.len()-4..])),
            );

            state
                .db
                .get_license()
                .map_err(|e| format!("Database error: {}", e))?
                .ok_or_else(|| "License activation failed".to_string())
        }
        Ok(resp) => {
            Err(resp.error.unwrap_or_else(|| "License activation rejected by server".to_string()))
        }
        Err(e) => {
            // Server unreachable — in dev mode, allow offline activation
            #[cfg(debug_assertions)]
            {
                log::warn!("License server unreachable ({}), activating offline in dev mode", e);
                let expires = chrono::Utc::now() + chrono::Duration::days(365);
                let expires_at = expires.to_rfc3339();

                state
                    .db
                    .activate_license(&key, &machine_id, &expires_at)
                    .map_err(|e| format!("Failed to store license: {}", e))?;

                return state
                    .db
                    .get_license()
                    .map_err(|e| format!("Database error: {}", e))?
                    .ok_or_else(|| "License activation failed".to_string());
            }

            #[cfg(not(debug_assertions))]
            Err(format!("Could not reach license server: {}. Please check your internet connection.", e))
        }
    }
}

#[tauri::command]
pub async fn deactivate_license(
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Notify server
    if let Ok(Some(lic)) = state.db.get_license() {
        let _ = call_deactivate_server(&lic.license_key, &lic.machine_id).await;
    }

    state
        .db
        .deactivate_license()
        .map_err(|e| format!("Failed to deactivate license: {}", e))?;

    let _ = state.db.insert_audit_log(None, "license_deactivated", None);

    Ok(())
}

#[tauri::command]
pub async fn check_license(
    #[allow(unused_variables)]
    state: State<'_, AppState>,
) -> Result<bool, String> {
    // MIT release — no license required
    Ok(true)
}

/// Called on app startup to validate the license with the server.
/// Updates local expiry and last_validated_at on success.
/// On failure, marks license as deactivated (unless within offline grace period).
#[tauri::command]
pub async fn validate_license_online(
    #[allow(unused_variables)]
    state: State<'_, AppState>,
) -> Result<bool, String> {
    // MIT release — no license validation required
    Ok(true)
}

// ─── HTTP helpers ───

async fn call_activate_server(key: &str, machine_id: &str) -> Result<ActivateResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let url = format!("{}/api/license/activate", LICENSE_SERVER_URL);
    let resp = client
        .post(&url)
        .json(&LicenseRequest {
            license_key: key.to_string(),
            machine_id: machine_id.to_string(),
        })
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    resp.json::<ActivateResponse>()
        .await
        .map_err(|e| format!("Invalid server response: {}", e))
}

#[cfg_attr(debug_assertions, allow(dead_code))]
async fn call_validate_server(key: &str, machine_id: &str) -> Result<ValidateResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let url = format!("{}/api/license/validate", LICENSE_SERVER_URL);
    let resp = client
        .post(&url)
        .json(&LicenseRequest {
            license_key: key.to_string(),
            machine_id: machine_id.to_string(),
        })
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    resp.json::<ValidateResponse>()
        .await
        .map_err(|e| format!("Invalid server response: {}", e))
}

async fn call_deactivate_server(key: &str, machine_id: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let url = format!("{}/api/license/deactivate", LICENSE_SERVER_URL);
    let _ = client
        .post(&url)
        .json(&LicenseRequest {
            license_key: key.to_string(),
            machine_id: machine_id.to_string(),
        })
        .send()
        .await;

    Ok(())
}

fn is_valid_license_format(key: &str) -> bool {
    let parts: Vec<&str> = key.split('-').collect();
    if parts.len() != 4 {
        return false;
    }
    if parts[0] != "QVLT" {
        return false;
    }
    parts[1..].iter().all(|p| p.len() == 4 && p.chars().all(|c| c.is_alphanumeric()))
}
