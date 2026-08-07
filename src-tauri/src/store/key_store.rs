//! Master encryption key storage: OS keychain (default when properly signed) or `.key` file.
//!
//! Backend selection:
//! - `DATAZEN_KEYRING=file` → always `.key` (dev / CI)
//! - `DATAZEN_KEYRING=keyring` → always OS keychain
//! - unset → OS keychain, except macOS adhoc/unsigned builds prefer `.key`
//!   (avoids Keychain ACL re-prompts after every `tauri:dev` re-link)

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::{StoreError, APP_IDENTIFIER};

pub const KEYRING_ACCOUNT: &str = "app-encryption-key";
const KEY_FILE: &str = ".key";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBackend {
    Keyring,
    File,
}

/// Resolve which backend stores the AES master key.
pub fn key_backend() -> KeyBackend {
    match std::env::var("DATAZEN_KEYRING").ok().as_deref() {
        Some("file") => KeyBackend::File,
        Some("keyring") => KeyBackend::Keyring,
        _ => {
            if should_prefer_file_backend() {
                KeyBackend::File
            } else {
                KeyBackend::Keyring
            }
        }
    }
}

fn should_prefer_file_backend() -> bool {
    #[cfg(target_os = "macos")]
    {
        static ADHOC: OnceLock<bool> = OnceLock::new();
        let adhoc = *ADHOC.get_or_init(macos_codesign_is_adhoc_or_unsigned);
        if adhoc {
            tracing::info!(
                "Using {KEY_FILE} key backend (macOS adhoc/unsigned binary); \
                 set DATAZEN_KEYRING=keyring to force OS Keychain"
            );
        }
        adhoc
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// True when the running binary is unsigned or adhoc-signed (typical for `tauri:dev`
/// and local unsigned `tauri:build` bundles). Proper Developer ID / App Store
/// signatures return false.
#[cfg(target_os = "macos")]
fn macos_codesign_is_adhoc_or_unsigned() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return true;
    };
    let path = std::fs::canonicalize(&exe).unwrap_or(exe);
    let output = std::process::Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=4"])
        .arg(&path)
        .output();
    let Ok(out) = output else {
        return true;
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    if text.contains("code object is not signed") {
        return true;
    }
    if text.contains("Signature=adhoc")
        || text.contains("flags=0x2(adhoc)")
        || text.contains("flags=0x2 (adhoc)")
    {
        return true;
    }
    false
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

/// Prefer `.key`. If missing and not in explicit `DATAZEN_KEYRING=file` mode,
/// try exporting from the OS keychain once (covers adhoc auto-fallback after a
/// prior keyring migration). Explicit file mode never touches Keychain so CI
/// and hermetic tests cannot hang on ACL prompts.
fn load_or_create_from_file(data_dir: &Path) -> Result<[u8; 32], StoreError> {
    if let Some(key) = read_key_file(data_dir)? {
        return Ok(key);
    }
    let explicit_file = std::env::var("DATAZEN_KEYRING").ok().as_deref() == Some("file");
    if !explicit_file {
        if let Ok(entry) = open_keyring_entry() {
            match entry.get_password() {
                Ok(key_b64) => {
                    let key = decode_key_b64(&key_b64)?;
                    write_key_file(data_dir, &key)?;
                    tracing::info!(
                        "Exported encryption key from OS keychain to {KEY_FILE} for file backend"
                    );
                    return Ok(key);
                }
                Err(keyring::v1::Error::NoEntry) => {}
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Could not read OS keychain while using file backend; creating {KEY_FILE} if needed"
                    );
                }
            }
        }
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
    use std::sync::Mutex;
    use tempfile::tempdir;

    /// `DATAZEN_KEYRING` is process-global; serialize tests that mutate it.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn file_backend_creates_key_when_missing() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("DATAZEN_KEYRING", "file");
        let dir = tempdir().unwrap();
        let key = load_or_create_master_key(dir.path()).unwrap();
        assert_ne!(key, [0u8; 32]);
        assert!(key_file_path(dir.path()).is_file());
    }

    #[test]
    fn file_backend_reloads_existing_key() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("DATAZEN_KEYRING", "file");
        let dir = tempdir().unwrap();
        let k1 = load_or_create_master_key(dir.path()).unwrap();
        let k2 = load_or_create_master_key(dir.path()).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn file_backend_does_not_invent_second_key_when_dot_key_exists() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("DATAZEN_KEYRING", "file");
        let dir = tempdir().unwrap();
        let known = [42u8; 32];
        write_key_file(dir.path(), &known).unwrap();
        let loaded = load_or_create_master_key(dir.path()).unwrap();
        assert_eq!(loaded, known);
        let on_disk = read_key_file(dir.path()).unwrap().unwrap();
        assert_eq!(on_disk, known);
    }

    #[test]
    fn env_file_forces_file_backend() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("DATAZEN_KEYRING", "file");
        assert_eq!(key_backend(), KeyBackend::File);
    }

    #[test]
    fn env_keyring_forces_keyring_backend() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("DATAZEN_KEYRING", "keyring");
        assert_eq!(key_backend(), KeyBackend::Keyring);
        std::env::set_var("DATAZEN_KEYRING", "file");
    }
}
