//! Master encryption key storage: OS keychain (default) or `.key` file fallback.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use std::path::{Path, PathBuf};

use super::{StoreError, APP_IDENTIFIER};

pub const KEYRING_ACCOUNT: &str = "app-encryption-key";
const KEY_FILE: &str = ".key";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBackend {
    Keyring,
    File,
}

/// `DATAZEN_KEYRING=file` forces the legacy `.key` file backend (CI/tests).
pub fn key_backend() -> KeyBackend {
    if std::env::var("DATAZEN_KEYRING").ok().as_deref() == Some("file") {
        KeyBackend::File
    } else {
        KeyBackend::Keyring
    }
}

pub fn key_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(KEY_FILE)
}

/// Load or create the 32-byte AES master key.
pub fn load_or_create_master_key(data_dir: &Path) -> Result<[u8; 32], StoreError> {
    match key_backend() {
        KeyBackend::File => load_or_create_from_file(data_dir),
        KeyBackend::Keyring => load_or_create_via_keyring(data_dir),
    }
}

fn decode_key_b64(key_b64: &str) -> Result<[u8; 32], StoreError> {
    let key_bytes = BASE64
        .decode(key_b64.trim())
        .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
    if key_bytes.len() != 32 {
        return Err(StoreError::EncryptionError("Invalid key length".into()));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);
    Ok(key)
}

fn generate_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

fn open_keyring_entry() -> Result<keyring::v1::Entry, keyring::v1::Error> {
    keyring::v1::Entry::new(APP_IDENTIFIER, KEYRING_ACCOUNT)
}

fn store_in_keyring(entry: &keyring::v1::Entry, key: &[u8; 32]) -> Result<(), keyring::v1::Error> {
    entry.set_password(&BASE64.encode(key))
}

fn read_key_file(data_dir: &Path) -> Result<Option<[u8; 32]>, StoreError> {
    let path = key_file_path(data_dir);
    let Ok(key_b64) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    decode_key_b64(&key_b64).map(Some)
}

fn write_key_file(data_dir: &Path, key: &[u8; 32]) -> Result<(), StoreError> {
    let path = key_file_path(data_dir);
    let key_b64 = BASE64.encode(key);
    std::fs::write(&path, key_b64.as_bytes())
        .map_err(|e| StoreError::EncryptionError(e.to_string()))
}

fn remove_key_file(data_dir: &Path) {
    let path = key_file_path(data_dir);
    if path.is_file() {
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!(path = %path.display(), error = %e, "Failed to delete legacy .key after keyring migration");
        }
    }
}

fn load_or_create_from_file(data_dir: &Path) -> Result<[u8; 32], StoreError> {
    if let Some(key) = read_key_file(data_dir)? {
        return Ok(key);
    }
    let key = generate_key();
    write_key_file(data_dir, &key)?;
    Ok(key)
}

fn fail_closed_no_key_source(context: &str) -> StoreError {
    StoreError::InitError(format!(
        "Cannot load encryption key: {context}. \
         Refusing to create a new key that would leave existing encrypted data unreadable."
    ))
}

fn load_legacy_key_or_fail(data_dir: &Path, context: &str) -> Result<[u8; 32], StoreError> {
    if let Some(key) = read_key_file(data_dir)? {
        tracing::warn!(
            "Using legacy {KEY_FILE} because {context}; \
             set DATAZEN_KEYRING=file to silence keychain attempts in CI"
        );
        Ok(key)
    } else {
        Err(fail_closed_no_key_source(&format!(
            "{context} and no legacy {KEY_FILE} file found"
        )))
    }
}

fn load_or_create_via_keyring(data_dir: &Path) -> Result<[u8; 32], StoreError> {
    let entry = match open_keyring_entry() {
        Ok(entry) => entry,
        Err(e) => {
            tracing::error!(error = %e, "Failed to open OS keychain entry for encryption key");
            return load_legacy_key_or_fail(
                data_dir,
                &format!("OS keychain unavailable ({e})"),
            );
        }
    };

    match entry.get_password() {
        Ok(key_b64) => {
            let key = decode_key_b64(&key_b64)?;
            remove_key_file(data_dir);
            Ok(key)
        }
        Err(keyring::v1::Error::NoEntry) => {
            if let Some(key) = read_key_file(data_dir)? {
                if let Err(e) = store_in_keyring(&entry, &key) {
                    tracing::warn!(
                        error = %e,
                        "Could not migrate .key to OS keychain; keeping file fallback"
                    );
                    return Ok(key);
                }
                remove_key_file(data_dir);
                Ok(key)
            } else {
                let key = generate_key();
                if let Err(e) = store_in_keyring(&entry, &key) {
                    tracing::warn!(
                        error = %e,
                        "Failed to store new encryption key in OS keychain; using file fallback"
                    );
                    write_key_file(data_dir, &key)?;
                }
                Ok(key)
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to read encryption key from OS keychain");
            load_legacy_key_or_fail(data_dir, &format!("OS keychain read failed ({e})"))
        }
    }
}

#[cfg(test)]
pub fn keyring_is_available() -> bool {
    if key_backend() == KeyBackend::File {
        return false;
    }
    matches!(keyring::v1::Entry::store_status(), Ok(()))
}

#[cfg(test)]
pub fn delete_keyring_entry_for_test() {
    if let Ok(entry) = open_keyring_entry() {
        let _ = entry.delete_credential();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn use_file_backend() {
        std::env::set_var("DATAZEN_KEYRING", "file");
    }

    #[test]
    fn file_backend_creates_key_when_missing() {
        use_file_backend();
        let dir = tempdir().unwrap();
        let key = load_or_create_master_key(dir.path()).unwrap();
        assert_ne!(key, [0u8; 32]);
        assert!(key_file_path(dir.path()).is_file());
    }

    #[test]
    fn file_backend_reloads_existing_key() {
        use_file_backend();
        let dir = tempdir().unwrap();
        let k1 = load_or_create_master_key(dir.path()).unwrap();
        let k2 = load_or_create_master_key(dir.path()).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn file_backend_does_not_invent_second_key_when_dot_key_exists() {
        use_file_backend();
        let dir = tempdir().unwrap();
        let known = [42u8; 32];
        write_key_file(dir.path(), &known).unwrap();
        let loaded = load_or_create_master_key(dir.path()).unwrap();
        assert_eq!(loaded, known);
        let on_disk = read_key_file(dir.path()).unwrap().unwrap();
        assert_eq!(on_disk, known);
    }
}
