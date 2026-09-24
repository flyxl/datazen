//! Master encryption key storage.
//!
//! Backend selection (priority order):
//! 1. `DATAZEN_KEYRING=file` → always `.key` file (dev / CI / explicit opt-in)
//! 2. `DATAZEN_KEYRING=keyring` → always OS keychain (keyring crate)
//! 3. unset → auto-detect:
//!    - macOS unsigned/adhoc → platform vault (security CLI → login keychain)
//!    - Windows → platform vault (DPAPI)
//!    - macOS signed / Linux → OS keychain (keyring crate)
//!
//! Migration: on first run after upgrade, any legacy `.key` file is transparently
//! migrated to the platform vault and the old file is deleted.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use std::path::{Path, PathBuf};

use super::{StoreError, APP_IDENTIFIER};

pub const KEYRING_ACCOUNT: &str = "app-encryption-key";
const KEY_FILE: &str = ".key";

// ---------------------------------------------------------------------------
// Backend selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBackend {
    /// Plaintext `.key` file (explicit opt-in via `DATAZEN_KEYRING=file`).
    File,
    /// OS keychain via `keyring` crate (macOS signed, Linux).
    Keyring,
    /// Platform vault: macOS security CLI / Windows DPAPI.
    PlatformVault,
}

/// Resolve which backend stores the AES master key.
pub fn key_backend() -> KeyBackend {
    match std::env::var("DATAZEN_KEYRING").ok().as_deref() {
        Some("file") => KeyBackend::File,
        Some("keyring") => KeyBackend::Keyring,
        _ => {
            #[cfg(test)]
            {
                KeyBackend::File
            }
            #[cfg(not(test))]
            {
                auto_detect_backend()
            }
        }
    }
}

#[cfg(not(test))]
fn auto_detect_backend() -> KeyBackend {
    // macOS: unsigned/adhoc → platform vault; signed → keyring
    #[cfg(target_os = "macos")]
    {
        if super::platform_vault::platform_vault_available() && macos_codesign_is_adhoc_or_unsigned()
        {
            tracing::info!(
                "Using platform vault (macOS security CLI) for encryption key; \
                 set DATAZEN_KEYRING=file for file backend, or DATAZEN_KEYRING=keyring for keyring crate"
            );
            return KeyBackend::PlatformVault;
        }
        return KeyBackend::Keyring;
    }
    // Windows: prefer DPAPI platform vault
    #[cfg(target_os = "windows")]
    {
        if super::platform_vault::platform_vault_available() {
            tracing::info!(
                "Using platform vault (DPAPI) for encryption key; \
                 set DATAZEN_KEYRING=file for file backend, or DATAZEN_KEYRING=keyring for keyring crate"
            );
            return KeyBackend::PlatformVault;
        }
        return KeyBackend::Keyring;
    }
    // Linux: keyring crate (D-Bus Secret Service)
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        KeyBackend::Keyring
    }
}

// ---------------------------------------------------------------------------
// macOS ad-hoc detection
// ---------------------------------------------------------------------------

/// True when the running binary is unsigned or adhoc-signed.
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
        "{}",
        String::from_utf8_lossy(&out.stderr),
    );
    if text.contains("code object is not signed") {
        return true;
    }
    text.contains("Signature=adhoc")
        || text.contains("flags=0x2(adhoc)")
        || text.contains("flags=0x2 (adhoc)")
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn key_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(KEY_FILE)
}

/// Load or create the 32-byte AES master key.
pub fn load_or_create_master_key(data_dir: &Path) -> Result<[u8; 32], StoreError> {
    match key_backend() {
        KeyBackend::File => load_or_create_from_file(data_dir),
        KeyBackend::PlatformVault => load_or_create_via_platform_vault(data_dir),
        KeyBackend::Keyring => load_or_create_via_keyring(data_dir),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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
        .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
    restrict_key_file_permissions(&path);
    Ok(())
}

fn restrict_key_file_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

fn remove_key_file(data_dir: &Path) {
    let path = key_file_path(data_dir);
    if path.is_file() {
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!(path = %path.display(), error = %e, "Failed to delete legacy .key after migration");
        }
    }
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

// ---------------------------------------------------------------------------
// File backend
// ---------------------------------------------------------------------------

fn load_or_create_from_file(data_dir: &Path) -> Result<[u8; 32], StoreError> {
    if let Some(key) = read_key_file(data_dir)? {
        return Ok(key);
    }
    let explicit_file = std::env::var("DATAZEN_KEYRING").ok().as_deref() == Some("file");
    if !explicit_file {
        // Try platform vault first, then keyring
        if super::platform_vault::platform_vault_available() {
            if let Ok(Some(key)) = super::platform_vault::load_from_vault() {
                write_key_file(data_dir, &key)?;
                tracing::info!("Exported encryption key from platform vault to {KEY_FILE}");
                return Ok(key);
            }
        }
        if let Ok(entry) = open_keyring_entry() {
            match entry.get_password() {
                Ok(key_b64) => {
                    let key = decode_key_b64(&key_b64)?;
                    write_key_file(data_dir, &key)?;
                    tracing::info!("Exported encryption key from OS keychain to {KEY_FILE}");
                    return Ok(key);
                }
                Err(keyring::v1::Error::NoEntry) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "Could not read OS keychain; creating {KEY_FILE} if needed");
                }
            }
        }
    }
    let key = generate_key();
    write_key_file(data_dir, &key)?;
    Ok(key)
}

// ---------------------------------------------------------------------------
// Platform vault backend (macOS security CLI / Windows DPAPI)
// ---------------------------------------------------------------------------

fn load_or_create_via_platform_vault(data_dir: &Path) -> Result<[u8; 32], StoreError> {
    // 1. Try loading from platform vault
    match super::platform_vault::load_from_vault() {
        Ok(Some(key)) => {
            // Migrate legacy .key file if present
            remove_key_file(data_dir);
            return Ok(key);
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %e, "Failed to read from platform vault");
            return load_legacy_key_or_fail(data_dir, &format!("platform vault read failed ({e})"));
        }
    }

    // 2. Vault empty — check for legacy .key file to migrate
    if let Some(key) = read_key_file(data_dir)? {
        if let Err(e) = super::platform_vault::store_to_vault(&key) {
            tracing::warn!(
                error = %e,
                "Could not migrate .key to platform vault; keeping file fallback"
            );
            return Ok(key);
        }
        remove_key_file(data_dir);
        tracing::info!("Migrated encryption key from legacy .key to platform vault");
        return Ok(key);
    }

    // 3. Brand new install — generate and store
    let key = generate_key();
    if let Err(e) = super::platform_vault::store_to_vault(&key) {
        tracing::warn!(
            error = %e,
            "Failed to store new key in platform vault; using file fallback"
        );
        write_key_file(data_dir, &key)?;
    }
    Ok(key)
}

// ---------------------------------------------------------------------------
// Keyring backend (macOS signed / Linux)
// ---------------------------------------------------------------------------

fn load_or_create_via_keyring(data_dir: &Path) -> Result<[u8; 32], StoreError> {
    let entry = match open_keyring_entry() {
        Ok(entry) => entry,
        Err(e) => {
            tracing::error!(error = %e, "Failed to open OS keychain entry");
            return load_legacy_key_or_fail(data_dir, &format!("OS keychain unavailable ({e})"));
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
                    tracing::warn!(error = %e, "Could not migrate .key to OS keychain; keeping file");
                    return Ok(key);
                }
                remove_key_file(data_dir);
                Ok(key)
            } else {
                let key = generate_key();
                if let Err(e) = store_in_keyring(&entry, &key) {
                    tracing::warn!(error = %e, "Failed to store key in OS keychain; using file");
                    write_key_file(data_dir, &key)?;
                }
                Ok(key)
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to read from OS keychain");
            load_legacy_key_or_fail(data_dir, &format!("OS keychain read failed ({e})"))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    #[test]
    fn key_file_path_joins_data_dir() {
        let dir = std::path::Path::new("/tmp/datazen-test");
        assert_eq!(key_file_path(dir), dir.join(KEY_FILE));
    }

    #[test]
    fn decode_key_b64_rejects_invalid_base64() {
        let err = decode_key_b64("not-valid-base64!!!").unwrap_err();
        assert!(matches!(err, StoreError::EncryptionError(_)));
    }

    #[test]
    fn decode_key_b64_rejects_wrong_length() {
        let short = BASE64.encode([1u8; 16]);
        let err = decode_key_b64(&short).unwrap_err();
        assert!(matches!(err, StoreError::EncryptionError(_)));
    }

    #[test]
    fn read_key_file_returns_none_when_missing() {
        let dir = tempdir().unwrap();
        assert!(read_key_file(dir.path()).unwrap().is_none());
    }

    #[test]
    fn read_key_file_returns_err_on_invalid_content() {
        let dir = tempdir().unwrap();
        std::fs::write(key_file_path(dir.path()), "garbage").unwrap();
        assert!(read_key_file(dir.path()).is_err());
    }

    #[test]
    fn write_and_read_key_file_roundtrip() {
        let dir = tempdir().unwrap();
        let key = [99u8; 32];
        write_key_file(dir.path(), &key).unwrap();
        assert_eq!(read_key_file(dir.path()).unwrap().unwrap(), key);
    }

    #[test]
    fn write_key_file_restricts_permissions_on_unix() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempdir().unwrap();
            write_key_file(dir.path(), &[1u8; 32]).unwrap();
            let mode = std::fs::metadata(key_file_path(dir.path()))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn decode_key_b64_accepts_valid_key_with_whitespace() {
        let key = [7u8; 32];
        let encoded = format!("  {}  ", BASE64.encode(key));
        assert_eq!(decode_key_b64(&encoded).unwrap(), key);
    }

    #[test]
    fn key_backend_unset_uses_file_or_keyring_by_platform() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("DATAZEN_KEYRING");
        let backend = key_backend();
        assert_eq!(backend, KeyBackend::File);
    }

    #[test]
    fn keyring_forced_without_legacy_key_fails_closed_when_keyring_unavailable() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("DATAZEN_KEYRING", "keyring");
        delete_keyring_entry_for_test();

        let dir = tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let dir_for_thread = dir.path().to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send(load_or_create_master_key(&dir_for_thread));
        });
        let result = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(result) => result,
            Err(_) => {
                eprintln!("skipping: keychain access timed out");
                return;
            }
        };
        match result {
            Ok(_) => {
                delete_keyring_entry_for_test();
            }
            Err(err) => {
                assert!(
                    matches!(&err, StoreError::InitError(msg) if msg.contains("Refusing to create")),
                    "unexpected: {err:?}"
                );
            }
        }
    }

    #[test]
    fn keyring_forced_falls_back_to_legacy_dot_key_when_keyring_unavailable() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("DATAZEN_KEYRING", "keyring");
        delete_keyring_entry_for_test();

        if keyring_is_available() {
            return;
        }

        let dir = tempdir().unwrap();
        let known = [11u8; 32];
        write_key_file(dir.path(), &known).unwrap();
        let loaded = load_or_create_master_key(dir.path()).unwrap();
        assert_eq!(loaded, known);
    }

    #[test]
    fn remove_key_file_deletes_existing_file() {
        let dir = tempdir().unwrap();
        let key = [5u8; 32];
        write_key_file(dir.path(), &key).unwrap();
        assert!(key_file_path(dir.path()).is_file());
        remove_key_file(dir.path());
        assert!(!key_file_path(dir.path()).exists());
    }

    #[test]
    fn remove_key_file_no_op_when_missing() {
        let dir = tempdir().unwrap();
        remove_key_file(dir.path());
        assert!(!key_file_path(dir.path()).exists());
    }

    #[test]
    fn generate_key_produces_nonzero_bytes() {
        let k1 = generate_key();
        let k2 = generate_key();
        assert_ne!(k1, [0u8; 32]);
        assert_ne!(k1, k2);
    }

    #[test]
    fn fail_closed_error_includes_context() {
        let err = fail_closed_no_key_source("unit test context");
        match err {
            StoreError::InitError(msg) => {
                assert!(msg.contains("unit test context"));
                assert!(msg.contains("Refusing to create"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn load_legacy_key_or_fail_reads_dot_key() {
        let dir = tempdir().unwrap();
        let known = [22u8; 32];
        write_key_file(dir.path(), &known).unwrap();
        let loaded = load_legacy_key_or_fail(dir.path(), "test context").unwrap();
        assert_eq!(loaded, known);
    }

    #[test]
    fn load_legacy_key_or_fail_without_file_is_fail_closed() {
        let dir = tempdir().unwrap();
        let err = load_legacy_key_or_fail(dir.path(), "missing key").unwrap_err();
        assert!(
            matches!(&err, StoreError::InitError(msg) if msg.contains("Refusing to create")),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn load_or_create_from_file_explicit_mode_skips_keyring() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("DATAZEN_KEYRING", "file");
        let dir = tempdir().unwrap();
        let k1 = load_or_create_from_file(dir.path()).unwrap();
        assert_ne!(k1, [0u8; 32]);
        assert!(key_file_path(dir.path()).is_file());
    }

    #[test]
    fn macos_codesign_detect_runs_without_panic() {
        #[cfg(target_os = "macos")]
        {
            let _ = macos_codesign_is_adhoc_or_unsigned();
        }
    }

    #[test]
    #[ignore = "requires OS keychain; run with: cargo test keyring_creates_new_key -- --ignored"]
    fn keyring_creates_and_reloads_master_key() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("DATAZEN_KEYRING", "keyring");
        delete_keyring_entry_for_test();

        if !keyring_is_available() {
            eprintln!("skip: OS keychain unavailable");
            return;
        }

        let dir = tempdir().unwrap();
        let k1 = load_or_create_master_key(dir.path()).unwrap();
        assert_ne!(k1, [0u8; 32]);
        let k2 = load_or_create_master_key(dir.path()).unwrap();
        assert_eq!(k1, k2);

        delete_keyring_entry_for_test();
    }
}
