//! Master encryption key storage.
//!
//! The backend is **not** decided here: the caller passes the
//! [`KeyBackend`] it resolved at startup (see [`super::key_backend`]). Backend
//! selection is a startup-time fact, so it must not be re-derived from the
//! process environment on the way to the key — otherwise any thread that
//! mutates `DATAZEN_KEYRING` could redirect a running store to the OS
//! keychain (macOS: modal authorization dialog, hangs the process).
//!
//! Migration: on first run after upgrade, any legacy `.key` file is transparently
//! migrated to the platform vault and the old file is deleted.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use std::path::{Path, PathBuf};

use super::key_backend::KeyBackend;
use super::{DesktopRepositoryError, APP_IDENTIFIER};

pub const KEYRING_ACCOUNT: &str = "app-encryption-key";
const KEY_FILE: &str = ".key";

#[cfg(test)]
use super::key_backend::test_keyring_opted_in;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn key_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(KEY_FILE)
}

/// Load or create the 32-byte AES master key from the given backend.
pub fn load_or_create_master_key(
    data_dir: &Path,
    backend: KeyBackend,
) -> Result<[u8; 32], DesktopRepositoryError> {
    match backend {
        KeyBackend::File => load_or_create_from_file(data_dir),
        KeyBackend::PlatformVault => load_or_create_via_platform_vault(data_dir),
        KeyBackend::Keyring => load_or_create_via_keyring(data_dir),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

use super::key_codec::decode_key_b64;

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

fn read_key_file(data_dir: &Path) -> Result<Option<[u8; 32]>, DesktopRepositoryError> {
    let path = key_file_path(data_dir);
    let Ok(key_b64) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    decode_key_b64(&key_b64).map(Some)
}

fn write_key_file(data_dir: &Path, key: &[u8; 32]) -> Result<(), DesktopRepositoryError> {
    let path = key_file_path(data_dir);
    let key_b64 = BASE64.encode(key);
    std::fs::write(&path, key_b64.as_bytes())
        .map_err(|e| DesktopRepositoryError::EncryptionError(e.to_string()))?;
    restrict_key_file_permissions(&path);
    Ok(())
}

/// Restrict `{appData}/.key` to the owning user — same intent as `mcp.token`
/// in [`crate::mcp::auth`].
///
/// - **Unix:** `chmod 600` (owner read/write only).
/// - **Windows:** not implemented yet — no shared ACL helper in the repo and
///   `mcp/auth.rs` applies the same Unix-only `chmod` for `mcp.token`. The file
///   inherits default user-profile ACLs; explicit DACL hardening is an open
///   follow-up.
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

fn fail_closed_no_key_source(context: &str) -> DesktopRepositoryError {
    DesktopRepositoryError::InitError(format!(
        "Cannot load encryption key: {context}. \
         Refusing to create a new key that would leave existing encrypted data unreadable."
    ))
}

fn load_legacy_key_or_fail(
    data_dir: &Path,
    context: &str,
) -> Result<[u8; 32], DesktopRepositoryError> {
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

fn load_or_create_from_file(data_dir: &Path) -> Result<[u8; 32], DesktopRepositoryError> {
    if let Some(key) = read_key_file(data_dir)? {
        return Ok(key);
    }
    // The file backend never probes the platform vault / OS keychain. The
    // backend is chosen explicitly by the caller (see `super::key_backend`),
    // and the documented migration direction is legacy `.key` → vault — see
    // `load_or_create_via_platform_vault` / `load_or_create_via_keyring`.
    let key = generate_key();
    write_key_file(data_dir, &key)?;
    Ok(key)
}

// ---------------------------------------------------------------------------
// Platform vault backend (macOS security CLI / Windows DPAPI)
// ---------------------------------------------------------------------------

fn load_or_create_via_platform_vault(data_dir: &Path) -> Result<[u8; 32], DesktopRepositoryError> {
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

fn load_or_create_via_keyring(data_dir: &Path) -> Result<[u8; 32], DesktopRepositoryError> {
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
pub fn keyring_is_available(backend: KeyBackend) -> bool {
    if backend == KeyBackend::File {
        return false;
    }
    matches!(keyring::v1::Entry::store_status(), Ok(()))
}

#[cfg(test)]
pub fn delete_keyring_entry_for_test() {
    // `delete_credential` is a real Keychain Services call, so it needs the same
    // opt-in as the tests that assert against the keychain backend.
    if !test_keyring_opted_in() {
        return;
    }
    if let Ok(entry) = open_keyring_entry() {
        let _ = entry.delete_credential();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Process-wide lock for every test that touches the process environment.
    ///
    /// `std::env::set_var` is process-global: a writer in one module races
    /// with a reader in another. The mutex lives in `crate::testing` so every
    /// writer **and** reader in the test binary shares one lock.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::testing::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn file_backend_creates_key_when_missing() {
        let dir = tempdir().unwrap();
        let key = load_or_create_master_key(dir.path(), KeyBackend::File).unwrap();
        assert_ne!(key, [0u8; 32]);
        assert!(key_file_path(dir.path()).is_file());
    }

    #[test]
    fn file_backend_reloads_existing_key() {
        let dir = tempdir().unwrap();
        let k1 = load_or_create_master_key(dir.path(), KeyBackend::File).unwrap();
        let k2 = load_or_create_master_key(dir.path(), KeyBackend::File).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn file_backend_does_not_invent_second_key_when_dot_key_exists() {
        let dir = tempdir().unwrap();
        let known = [42u8; 32];
        write_key_file(dir.path(), &known).unwrap();
        let loaded = load_or_create_master_key(dir.path(), KeyBackend::File).unwrap();
        assert_eq!(loaded, known);
        let on_disk = read_key_file(dir.path()).unwrap().unwrap();
        assert_eq!(on_disk, known);
    }

    #[test]
    fn key_file_path_joins_data_dir() {
        let dir = std::path::Path::new("/tmp/datazen-test");
        assert_eq!(key_file_path(dir), dir.join(KEY_FILE));
    }

    #[test]
    fn decode_key_b64_rejects_invalid_base64() {
        let err = decode_key_b64("not-valid-base64!!!").unwrap_err();
        assert!(matches!(err, DesktopRepositoryError::EncryptionError(_)));
    }

    #[test]
    fn decode_key_b64_rejects_wrong_length() {
        let short = BASE64.encode([1u8; 16]);
        let err = decode_key_b64(&short).unwrap_err();
        assert!(matches!(err, DesktopRepositoryError::EncryptionError(_)));
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
    fn keyring_forced_without_legacy_key_fails_closed_when_keyring_unavailable() {
        let _guard = env_lock();
        if !test_keyring_opted_in() {
            eprintln!("skipping: set DATAZEN_TEST_KEYRING=1 to exercise the OS keychain");
            return;
        }
        delete_keyring_entry_for_test();

        let dir = tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let dir_for_thread = dir.path().to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send(load_or_create_master_key(
                &dir_for_thread,
                KeyBackend::Keyring,
            ));
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
                    matches!(&err, DesktopRepositoryError::InitError(msg) if msg.contains("Refusing to create")),
                    "unexpected: {err:?}"
                );
            }
        }
    }

    #[test]
    fn keyring_forced_falls_back_to_legacy_dot_key_when_keyring_unavailable() {
        let _guard = env_lock();
        if !test_keyring_opted_in() {
            eprintln!("skipping: set DATAZEN_TEST_KEYRING=1 to exercise the OS keychain");
            return;
        }
        delete_keyring_entry_for_test();

        if keyring_is_available(KeyBackend::Keyring) {
            return;
        }

        let dir = tempdir().unwrap();
        let known = [11u8; 32];
        write_key_file(dir.path(), &known).unwrap();
        let loaded = load_or_create_master_key(dir.path(), KeyBackend::Keyring).unwrap();
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
            DesktopRepositoryError::InitError(msg) => {
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
            matches!(&err, DesktopRepositoryError::InitError(msg) if msg.contains("Refusing to create")),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn load_or_create_from_file_explicit_mode_skips_keyring() {
        let dir = tempdir().unwrap();
        let k1 = load_or_create_from_file(dir.path()).unwrap();
        assert_ne!(k1, [0u8; 32]);
        assert!(key_file_path(dir.path()).is_file());
    }

    #[test]
    #[ignore = "requires OS keychain; run with: DATAZEN_TEST_KEYRING=1 cargo test keyring_creates -- --ignored"]
    fn keyring_creates_and_reloads_master_key() {
        let _guard = env_lock();
        if !test_keyring_opted_in() {
            eprintln!("skipping: set DATAZEN_TEST_KEYRING=1 to exercise the OS keychain");
            return;
        }
        // The only place left that may write the process env: this test runs on
        // its own (`cargo test -- --ignored`) and exercises the startup
        // resolution end to end.
        let _keyring = crate::testing::ForceKeyringGuard::new();
        let backend = super::super::key_backend::resolve_key_backend();
        assert_eq!(backend, KeyBackend::Keyring);
        delete_keyring_entry_for_test();

        if !keyring_is_available(backend) {
            eprintln!("skip: OS keychain unavailable");
            return;
        }

        let dir = tempdir().unwrap();
        let k1 = load_or_create_master_key(dir.path(), backend).unwrap();
        assert_ne!(k1, [0u8; 32]);
        let k2 = load_or_create_master_key(dir.path(), backend).unwrap();
        assert_eq!(k1, k2);

        delete_keyring_entry_for_test();
    }
}
