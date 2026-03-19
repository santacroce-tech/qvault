use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use ring::digest;
use std::fs;
use std::path::Path;

/// Derive a 256-bit key from a seed string using SHA-256
fn derive_key(seed: &str) -> [u8; 32] {
    let hash = digest::digest(&digest::SHA256, seed.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(hash.as_ref());
    key
}

/// Get a machine-specific encryption key seed
fn machine_seed() -> String {
    // Use a combination of factors for the seed
    // In production, this would use machine-uid crate + OS keychain
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "qvault-default".to_string());
    format!("qvault-encryption-{}", hostname)
}

pub fn encrypt_file(source: &Path, dest: &Path) -> Result<(), String> {
    let data = fs::read(source).map_err(|e| format!("Failed to read file: {}", e))?;

    let key_bytes = derive_key(&machine_seed());
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| format!("Failed to create cipher: {}", e))?;

    // Generate random 96-bit nonce
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted = cipher
        .encrypt(nonce, data.as_ref())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Write: nonce (12 bytes) + ciphertext
    let mut output = Vec::with_capacity(12 + encrypted.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&encrypted);

    fs::write(dest, &output).map_err(|e| format!("Failed to write encrypted file: {}", e))?;

    Ok(())
}

pub fn decrypt_file(source: &Path) -> Result<Vec<u8>, String> {
    let data = fs::read(source).map_err(|e| format!("Failed to read encrypted file: {}", e))?;

    if data.len() < 12 {
        return Err("Encrypted file too short".to_string());
    }

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key_bytes = derive_key(&machine_seed());
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| format!("Failed to create cipher: {}", e))?;

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))
}
