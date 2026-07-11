//! Encryption at Rest (Odysseus Fernet inspired)
//!
//! Provides transparent encryption/decryption for sensitive fields
//! (API keys, tokens, passwords) stored in SQLite.
//!
//! Uses AES-256-GCM via a device-derived key. The encryption key is:
//! 1. Generated using OS CSPRNG (getrandom) on first run
//! 2. Stored in ~/.omnix/.encryption_key (hex-encoded, file permissions restricted)
//! 3. Used to encrypt/decrypt all sensitive fields transparently
//!
//! Encrypted format: "ENC:v2:<base64(nonce || ciphertext || tag)>"
//! The v2 prefix distinguishes from legacy XOR-encrypted values for migration.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use log::warn;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Encryption prefix marker — v2 uses AES-256-GCM
const ENCRYPTED_PREFIX_V2: &str = "ENC:v2:";
/// Legacy XOR prefix (for backward compatibility during migration)
const ENCRYPTED_PREFIX_V1: &str = "ENC:";

/// Global encryption key (initialized once)
static ENCRYPTION_KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// Get or generate the encryption key using OS CSPRNG
fn get_key() -> &'static [u8; 32] {
    ENCRYPTION_KEY.get_or_init(|| {
        let key_path = key_path();

        if key_path.exists() {
            // Read existing key
            if let Ok(hex) = fs::read_to_string(&key_path) {
                if let Some(bytes) = hex_to_bytes(hex.trim()) {
                    if bytes.len() == 32 {
                        let mut key = [0u8; 32];
                        key.copy_from_slice(&bytes);
                        return key;
                    }
                }
            }
        }

        // Generate new key using OS CSPRNG (getrandom)
        let mut key = [0u8; 32];
        getrandom::getrandom(&mut key)
            .expect("[crypto] FATAL: OS CSPRNG (getrandom) failed — cannot securely generate encryption key. This should never happen on a modern OS.");

        // Save key to file with restricted permissions
        let hex_key = bytes_to_hex(&key);
        let parent = key_path.parent().expect("key_path should have a parent directory");
        let _ = fs::create_dir_all(parent);
        let _ = fs::write(&key_path, &hex_key);

        // On Unix, restrict key file to owner-only (0o600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600));
        }

        // On Windows, restrict key file to current user only using DACL
        #[cfg(windows)]
        {
            // Windows file security: remove inherited permissions and grant
            // full control only to the current user. This prevents other users
            // on the same machine from reading the encryption key.
            let _ = restrict_key_file_windows(&key_path);
        }

        key
    })
}

fn key_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    home.join(".omnix").join(".encryption_key")
}

/// Encrypt a string value using AES-256-GCM.
/// Returns "ENC:v2:<base64(nonce || ciphertext_with_tag)>"
pub fn encrypt(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }

    let key = get_key();
    let cipher = Aes256Gcm::new_from_slice(key).expect("AES-256-GCM key is always 32 bytes");

    // Generate a random 96-bit nonce (required for AES-GCM)
    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes)
        .expect("getrandom should not fail on supported platforms");
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt: AES-256-GCM produces ciphertext || 16-byte auth tag
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .expect("AES-256-GCM encryption should not fail for valid input");

    // Concatenate nonce || ciphertext (ciphertext already includes auth tag)
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    format!("{}{}", ENCRYPTED_PREFIX_V2, B64.encode(&combined))
}

/// Decrypt a value.
/// - If it starts with "ENC:v2:", decrypt using AES-256-GCM
/// - If it starts with "ENC:" (legacy), decrypt using XOR (backward compat)
/// - Otherwise return as-is (plaintext passthrough)
pub fn decrypt(value: &str) -> String {
    if value.starts_with(ENCRYPTED_PREFIX_V2) {
        decrypt_v2(value)
    } else if value.starts_with(ENCRYPTED_PREFIX_V1) {
        decrypt_v1_legacy(value)
    } else {
        value.to_string()
    }
}

/// Decrypt AES-256-GCM encrypted value
fn decrypt_v2(value: &str) -> String {
    let payload_b64 = &value[ENCRYPTED_PREFIX_V2.len()..];
    let payload = match B64.decode(payload_b64) {
        Ok(v) => v,
        Err(_) => return value.to_string(), // Invalid base64, return as-is
    };

    // Need at least 12 bytes nonce + 16 bytes tag + some ciphertext
    if payload.len() < 12 + 16 {
        return value.to_string();
    }

    let (nonce_bytes, ciphertext) = payload.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = get_key();
    let cipher = Aes256Gcm::new_from_slice(key).expect("AES-256-GCM key is always 32 bytes");

    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext_bytes) => String::from_utf8(plaintext_bytes)
            .unwrap_or_else(|_| value.to_string()),
        Err(_) => {
            // Decryption failed (tampered data or wrong key)
            warn!("AES-256-GCM decryption failed — data may be tampered or key changed");
            value.to_string()
        }
    }
}

/// Legacy XOR decryption for backward compatibility with v1 encrypted values.
/// This handles values encrypted by the previous XOR-based implementation.
fn decrypt_v1_legacy(value: &str) -> String {
    let ciphertext_b64 = &value[ENCRYPTED_PREFIX_V1.len()..];
    let ciphertext = match base64_decode_simple(ciphertext_b64) {
        Some(v) => v,
        None => return value.to_string(),
    };

    let key = get_key();
    let mut decrypted = Vec::with_capacity(ciphertext.len());
    for (i, &b) in ciphertext.iter().enumerate() {
        let key_byte = key[i % 32];
        let nonce_byte = key[(i * 7 + 13) % 32];
        decrypted.push(b ^ key_byte ^ nonce_byte);
    }

    String::from_utf8(decrypted).unwrap_or_else(|_| value.to_string())
}

/// Check if a value is encrypted (either v1 or v2 format)
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX_V2) || value.starts_with(ENCRYPTED_PREFIX_V1)
}

/// Windows-specific: restrict key file to current user only.
/// Uses icacls to remove inherited permissions and grant full control
/// only to the current user. Silently fails if icacls is unavailable.
#[cfg(windows)]
fn restrict_key_file_windows(path: &std::path::Path) -> Result<(), String> {
    use crate::proc::NoWindow;
    let path_str = path.to_string_lossy().to_string();
    // Remove inherited permissions and grant current user full control only
    let output = std::process::Command::new("icacls")
        .arg(&path_str)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{}:(F)", whoami::username()))
        .no_window()
        .output()
        .map_err(|e| format!("Failed to run icacls: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Non-critical: log but don't fail startup
        warn!("icacls failed for key file failed for key file: {}", stderr);
    }
    Ok(())
}

/// Migrate a v1 (XOR) encrypted value to v2 (AES-256-GCM).
/// Returns the re-encrypted value in v2 format, or the original if not v1.
pub fn migrate_v1_to_v2(value: &str) -> String {
    if !value.starts_with(ENCRYPTED_PREFIX_V1) || value.starts_with(ENCRYPTED_PREFIX_V2) {
        return value.to_string(); // Not v1, nothing to migrate
    }
    let plaintext = decrypt_v1_legacy(value);
    encrypt(&plaintext)
}

// ── Utility functions ──

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16).ok()?;
        bytes.push(byte);
    }
    Some(bytes)
}

/// Simple base64 decode for legacy v1 values (no external dependency needed)
fn base64_decode_simple(data: &str) -> Option<Vec<u8>> {
    let data: Vec<u8> = data.bytes().filter(|b| *b != b'\n' && *b != b'\r').collect();
    let mut result = Vec::with_capacity(data.len() * 3 / 4);

    for chunk in data.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let a = char_to_val(chunk[0])? as u32;
        let b = char_to_val(chunk[1])? as u32;
        let c = if chunk.len() > 2 && chunk[2] != b'=' {
            char_to_val(chunk[2])? as u32
        } else {
            0
        };
        let d = if chunk.len() > 3 && chunk[3] != b'=' {
            char_to_val(chunk[3])? as u32
        } else {
            0
        };

        let triple = (a << 18) | (b << 12) | (c << 6) | d;
        result.push(((triple >> 16) & 0xFF) as u8);
        if chunk.len() > 2 && chunk[2] != b'=' {
            result.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk.len() > 3 && chunk[3] != b'=' {
            result.push((triple & 0xFF) as u8);
        }
    }
    Some(result)
}

fn char_to_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let plaintext = "sk-abc123-secret-api-key";
        let encrypted = encrypt(plaintext);
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX_V2));
        assert!(is_encrypted(&encrypted));
        let decrypted = decrypt(&encrypted);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_plaintext_passthrough() {
        let plaintext = "not-encrypted-value";
        assert!(!is_encrypted(plaintext));
        assert_eq!(decrypt(plaintext), plaintext);
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(encrypt(""), "");
        assert_eq!(decrypt(""), "");
    }

    #[test]
    fn test_special_chars() {
        let plaintext = "key with spaces & special chars: !@#$%^&*()";
        let encrypted = encrypt(plaintext);
        let decrypted = decrypt(&encrypted);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_unicode() {
        let plaintext = "密钥-中文-🔑-emoji";
        let encrypted = encrypt(plaintext);
        let decrypted = decrypt(&encrypted);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_encryptions_differ() {
        // Same plaintext should produce different ciphertext (random nonce)
        let plaintext = "same-input";
        let enc1 = encrypt(plaintext);
        let enc2 = encrypt(plaintext);
        assert_ne!(enc1, enc2); // Different nonces → different ciphertext
        assert_eq!(decrypt(&enc1), decrypt(&enc2)); // But both decrypt correctly
    }

    #[test]
    fn test_v1_backward_compat() {
        // Simulate a v1-encrypted value by manually XOR-encrypting
        let key = get_key();
        let plaintext = "legacy-api-key";
        let bytes = plaintext.as_bytes();
        let mut encrypted = Vec::with_capacity(bytes.len());
        for (i, &b) in bytes.iter().enumerate() {
            let key_byte = key[i % 32];
            let nonce_byte = key[(i * 7 + 13) % 32];
            encrypted.push(b ^ key_byte ^ nonce_byte);
        }
        let v1_value = format!("{}{}", ENCRYPTED_PREFIX_V1, base64_encode_simple(&encrypted));

        // Should decrypt correctly via v1 legacy path
        let decrypted = decrypt(&v1_value);
        assert_eq!(decrypted, plaintext);
    }

    fn base64_encode_simple(data: &[u8]) -> String {
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        B64.encode(data)
    }
}
