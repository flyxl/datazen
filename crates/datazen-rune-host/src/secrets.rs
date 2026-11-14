//! The host's password encryption key (T10): the 32-byte AES master key the
//! connection store encrypts passwords with, and where it lives. The
//! semantics are the desktop `key_backend`/`key_store` ladder, host-scoped:
//!
//! 1. `DATAZEN_NATIVE_KEYRING=file` → a `.key` file beside the store
//!    (0600) — the headless and CI escape hatch.
//! 2. unset / `keyring` → the OS keychain via the `keyring` crate (service
//!    `datazen-native`, account `app-encryption-key`). A keyring that
//!    cannot be reached falls back to a legacy `.key` file when one exists;
//!    with neither, loading fails closed — a new key would leave existing
//!    encrypted passwords unreadable.
//!
//! The backend is resolved once per key load, not per call: the environment
//! cannot flip a running store's backend underneath a caller.

use std::path::{Path, PathBuf};

/// The keyring entry the master key lives in.
pub const KEYRING_SERVICE: &str = "datazen-native";
pub const KEYRING_ACCOUNT: &str = "app-encryption-key";
/// The file backend's file, beside the connection store.
const KEY_FILE: &str = ".key";

/// Where the master key lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBackend {
    /// A plaintext `.key` file (explicit opt-in).
    File,
    /// The OS keychain via the `keyring` crate.
    Keyring,
}

/// Resolve the backend from the environment once. Unset and unrecognized
/// values select the keyring — the desktop's default on Linux.
pub fn resolve_key_backend() -> KeyBackend {
    match std::env::var("DATAZEN_NATIVE_KEYRING").as_deref() {
        Ok("file") => KeyBackend::File,
        _ => KeyBackend::Keyring,
    }

    // (kept as a statement block so the match reads as the whole rule)
}

/// The pure mapping, so the rule itself is pinnable by a test.
pub fn backend_from_env_value(value: Option<&str>) -> KeyBackend {
    match value {
        Some("file") => KeyBackend::File,
        _ => KeyBackend::Keyring,
    }
}

/// Load or create the 32-byte master key from the given backend.
pub fn load_or_create_master_key(
    data_dir: &Path,
    backend: KeyBackend,
) -> Result<[u8; 32], String> {
    match backend {
        KeyBackend::File => load_or_create_from_file(data_dir),
        KeyBackend::Keyring => load_or_create_via_keyring(data_dir),
    }
}

fn key_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(KEY_FILE)
}

fn read_key_file(data_dir: &Path) -> Result<Option<[u8; 32]>, String> {
    let Ok(key_b64) = std::fs::read_to_string(key_file_path(data_dir)) else {
        return Ok(None);
    };
    decode_key_b64(&key_b64).map(Some)
}

fn write_key_file(data_dir: &Path, key: &[u8; 32]) -> Result<(), String> {
    use base64::Engine;
    let path = key_file_path(data_dir);
    std::fs::write(&path, base64::engine::general_purpose::STANDARD.encode(key))
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn remove_key_file(data_dir: &Path) {
    let path = key_file_path(data_dir);
    if path.is_file() {
        let _ = std::fs::remove_file(&path);
    }
}

fn generate_key() -> [u8; 32] {
    use rand::RngCore;
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

fn decode_key_b64(value: &str) -> Result<[u8; 32], String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|error| format!("the stored key is not valid base64: {error}"))?;
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        format!("the stored key is {} bytes, not 32", bytes.len())
    })
}

fn load_or_create_from_file(data_dir: &Path) -> Result<[u8; 32], String> {
    if let Some(key) = read_key_file(data_dir)? {
        return Ok(key);
    }
    let key = generate_key();
    write_key_file(data_dir, &key)?;
    Ok(key)
}

fn load_or_create_via_keyring(data_dir: &Path) -> Result<[u8; 32], String> {
    let entry = match keyring::v1::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(entry) => entry,
        Err(error) => {
            return legacy_key_or_fail(data_dir, &format!("the OS keychain is unavailable ({error})"))
        }
    };
    match entry.get_password() {
        Ok(key_b64) => {
            let key = decode_key_b64(&key_b64)?;
            // The keyring answered: a legacy file key is no longer needed.
            remove_key_file(data_dir);
            Ok(key)
        }
        Err(keyring::v1::Error::NoEntry) => {
            if let Some(key) = read_key_file(data_dir)? {
                // Migrate the legacy file key into the keyring best-effort;
                // the file stays when the keyring will not take it.
                if entry.set_password(&encode_key_b64(&key)).is_ok() {
                    remove_key_file(data_dir);
                }
                return Ok(key);
            }
            let key = generate_key();
            if entry.set_password(&encode_key_b64(&key)).is_err() {
                // A keyring that cannot be written falls to the file, so a
                // fresh install still works headless with the escape hatch.
                write_key_file(data_dir, &key)?;
            }
            Ok(key)
        }
        Err(error) => legacy_key_or_fail(
            data_dir,
            &format!("the OS keychain read failed ({error})"),
        ),
    }
}

fn encode_key_b64(key: &[u8; 32]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(key)
}

/// The keyring failed: a legacy `.key` file is the documented escape hatch;
/// with neither, fail closed — a new key would leave existing encrypted
/// passwords unreadable.
fn legacy_key_or_fail(data_dir: &Path, context: &str) -> Result<[u8; 32], String> {
    if let Some(key) = read_key_file(data_dir)? {
        return Ok(key);
    }
    Err(format!(
        "the password encryption key is unavailable: {context}; \
         set DATAZEN_NATIVE_KEYRING=file to use a key file"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("datazen-rune-secrets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_backend_mapping_is_the_desktops_ladder() {
        assert_eq!(backend_from_env_value(Some("file")), KeyBackend::File);
        assert_eq!(backend_from_env_value(Some("keyring")), KeyBackend::Keyring);
        assert_eq!(backend_from_env_value(None), KeyBackend::Keyring);
        assert_eq!(backend_from_env_value(Some("nonsense")), KeyBackend::Keyring);
    }

    #[test]
    fn the_file_backend_round_trips_and_restricts_the_file() {
        let dir = temp_dir("file-backend");
        let key = load_or_create_master_key(&dir, KeyBackend::File).unwrap();
        assert_eq!(key.len(), 32);
        // The same key comes back on the next load.
        assert_eq!(
            load_or_create_master_key(&dir, KeyBackend::File).unwrap(),
            key,
            "the key file is stable"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(key_file_path(&dir))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "the key file is owner-only");
        }
    }

    #[test]
    fn a_keyring_that_cannot_be_reached_fails_closed_without_a_legacy_key() {
        // This sandbox runs no session bus: the keyring backend cannot be
        // reached. Without a legacy .key file the load must fail closed —
        // an honest sentence, never a silently fresh key.
        let dir = temp_dir("keyring-closed");
        let error = load_or_create_master_key(&dir, KeyBackend::Keyring)
            .expect_err("no bus and no key file is a closed failure");
        assert!(
            error.contains("DATAZEN_NATIVE_KEYRING=file"),
            "the sentence names the escape hatch: {error}"
        );
        assert!(!key_file_path(&dir).exists(), "no key file appeared");

        // The legacy .key file is the escape hatch: with one, the keyring
        // backend loads it.
        let key = load_or_create_master_key(&dir, KeyBackend::File).unwrap();
        let rescued = load_or_create_master_key(&dir, KeyBackend::Keyring).unwrap();
        assert_eq!(rescued, key, "the legacy key file answers when the keyring cannot");
    }
}
