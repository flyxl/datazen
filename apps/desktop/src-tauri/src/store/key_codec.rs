//! Shared decoding for the AES master key.
//!
//! The master key travels as base64 between the key store and the platform
//! vault (macOS `security` CLI, Windows DPAPI). Both sides have to decode it
//! the same way — a second copy is a second opinion about the same key — so
//! the decoding lives here once.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

use crate::store::DesktopRepositoryError;

/// Decode a base64 master key into its 32 raw bytes.
pub(super) fn decode_key_b64(key_b64: &str) -> Result<[u8; 32], DesktopRepositoryError> {
    let key_bytes = BASE64
        .decode(key_b64.trim())
        .map_err(|e| DesktopRepositoryError::EncryptionError(e.to_string()))?;
    if key_bytes.len() != 32 {
        return Err(DesktopRepositoryError::EncryptionError(
            "Invalid key length".into(),
        ));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);
    Ok(key)
}
