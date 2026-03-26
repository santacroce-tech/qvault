use std::fs;
use std::path::Path;

/// Copy file to storage (no encryption for MIT release)
pub fn encrypt_file(source: &Path, dest: &Path) -> Result<(), String> {
    fs::copy(source, dest)
        .map_err(|e| format!("Failed to copy file: {}", e))?;
    Ok(())
}

/// Read file from storage (no decryption for MIT release)
pub fn decrypt_file(source: &Path) -> Result<Vec<u8>, String> {
    fs::read(source)
        .map_err(|e| format!("Failed to read file: {}", e))
}
