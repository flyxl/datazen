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
    backend_from_env_value(std::env::var("DATAZEN_NATIVE_KEYRING").as_deref().ok())
}

/// The pure mapping, so the rule itself is pinnable by a test.
pub fn backend_from_env_value(value: Option<&str>) -> KeyBackend {
    match value {
        Some("file") => KeyBackend::File,
        _ => KeyBackend::Keyring,
    }
}

/// Load or create the 32-byte master key from the given backend.
pub fn load_or_create_master_key(data_dir: &Path, backend: KeyBackend) -> Result<[u8; 32], String> {
    load_master_key(data_dir, backend, true)
}

/// When a profile may already contain ciphertext, only an existing key may be used.
pub(crate) fn load_master_key(
    data_dir: &Path,
    backend: KeyBackend,
    allow_create: bool,
) -> Result<[u8; 32], String> {
    match backend {
        KeyBackend::File => load_from_file(data_dir, allow_create),
        KeyBackend::Keyring => load_or_create_via_keyring(
            data_dir,
            keyring::v1::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT),
            allow_create,
        ),
    }
}

fn key_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(KEY_FILE)
}

fn read_key_file(data_dir: &Path) -> Result<Option<[u8; 32]>, String> {
    match std::fs::read_to_string(key_file_path(data_dir)) {
        Ok(key) => decode_key_b64(&key).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("the password key file is unreadable".into()),
    }
}

fn create_key_file(data_dir: &Path) -> Result<[u8; 32], String> {
    use std::io::Write;
    let key = generate_key();
    let write_error = |_| "could not create the password key file".to_string();
    std::fs::create_dir_all(data_dir).map_err(write_error)?;
    // A NamedTempFile starts owner-only on Unix. Publish a fully written key
    // without replacing a key another process may have installed meanwhile.
    let mut tmp = tempfile::NamedTempFile::new_in(data_dir).map_err(write_error)?;
    tmp.write_all(encode_key_b64(&key).as_bytes())
        .map_err(write_error)?;
    tmp.as_file().sync_all().map_err(write_error)?;
    match tmp.persist_noclobber(key_file_path(data_dir)) {
        Ok(_) => Ok(key),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_key_file(data_dir)?.ok_or_else(|| "the password key file disappeared".into())
        }
        Err(_) => Err("could not create the password key file".into()),
    }
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
        .map_err(|_| "the stored password key is invalid")?;
    bytes
        .try_into()
        .map_err(|_| "the stored password key is invalid".into())
}

const MISSING_KEY: &str = "restore the original password encryption key";

fn load_from_file(data_dir: &Path, allow_create: bool) -> Result<[u8; 32], String> {
    match read_key_file(data_dir)? {
        Some(key) => Ok(key),
        None if allow_create => create_key_file(data_dir),
        None => Err(MISSING_KEY.into()),
    }
}

fn load_or_create_via_keyring(
    data_dir: &Path,
    entry: keyring::v1::Result<keyring::v1::Entry>,
    allow_create: bool,
) -> Result<[u8; 32], String> {
    let entry = match entry {
        Ok(entry) => entry,
        Err(_) => return legacy_key_or_fail(data_dir),
    };
    resolve_keyring_key(data_dir, entry.get_password(), allow_create, |key| {
        entry.set_password(key)
    })
}

fn resolve_keyring_key(
    data_dir: &Path,
    read: keyring::v1::Result<String>,
    allow_create: bool,
    store: impl FnOnce(&str) -> keyring::v1::Result<()>,
) -> Result<[u8; 32], String> {
    match read {
        Ok(key_b64) => {
            let key = decode_key_b64(&key_b64)?;
            // A different legacy file key may still be needed for recovery.
            if matches!(read_key_file(data_dir), Ok(Some(file_key)) if file_key == key) {
                remove_key_file(data_dir);
            }
            Ok(key)
        }
        Err(keyring::v1::Error::NoEntry) => {
            if let Some(key) = read_key_file(data_dir)? {
                // Migrate the legacy file key into the keyring best-effort;
                // the file stays when the keyring will not take it.
                if store(&encode_key_b64(&key)).is_ok() {
                    remove_key_file(data_dir);
                }
                return Ok(key);
            }
            if !allow_create {
                return Err(MISSING_KEY.into());
            }
            let key = generate_key();
            store(&encode_key_b64(&key)).map_err(|_| {
                "could not store the password encryption key in the OS keychain; configure DATAZEN_NATIVE_KEYRING=file to opt into a key file".to_string()
            })?;
            Ok(key)
        }
        Err(_) => legacy_key_or_fail(data_dir),
    }
}

fn encode_key_b64(key: &[u8; 32]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(key)
}

/// The keyring failed: a legacy `.key` file is the documented escape hatch;
/// with neither, fail closed — a new key would leave existing encrypted
/// passwords unreadable.
fn legacy_key_or_fail(data_dir: &Path) -> Result<[u8; 32], String> {
    if let Some(key) = read_key_file(data_dir)? {
        return Ok(key);
    }
    Err("the OS keychain is unavailable; restore the keychain or use the original .key with DATAZEN_NATIVE_KEYRING=file".into())
}

#[cfg(test)]
pub(crate) fn unavailable_keyring(data_dir: &Path, allow_create: bool) -> Result<[u8; 32], String> {
    load_or_create_via_keyring(
        data_dir,
        Err(keyring::v1::Error::NoDefaultStore),
        allow_create,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("datazen-rune-secrets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_backend_mapping_is_the_desktops_ladder() {
        assert_eq!(backend_from_env_value(Some("file")), KeyBackend::File);
        assert_eq!(backend_from_env_value(Some("keyring")), KeyBackend::Keyring);
        assert_eq!(backend_from_env_value(None), KeyBackend::Keyring);
        assert_eq!(
            backend_from_env_value(Some("nonsense")),
            KeyBackend::Keyring
        );
    }

    #[test]
    fn the_file_backend_round_trips_and_restricts_the_file() {
        let dir = temp_dir("file-backend");
        let key = load_or_create_master_key(&dir, KeyBackend::File).unwrap();
        assert_eq!(key.len(), 32);
        // The same key comes back on the next load.
        assert!(load_or_create_master_key(&dir, KeyBackend::File).unwrap() == key);
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
    fn an_empty_keychain_cannot_silently_replace_a_missing_key_or_opt_in_to_a_file() {
        use keyring::v1::Error;
        let dir = tempfile::tempdir().unwrap();
        let missing = resolve_keyring_key(dir.path(), Err(Error::NoEntry), false, |_| {
            panic!("an existing protected store must not create a key")
        });
        assert!(missing.is_err());
        let unwritable = resolve_keyring_key(dir.path(), Err(Error::NoEntry), true, |_| {
            Err(Error::NoDefaultStore)
        });
        assert!(unwritable.is_err());
        assert!(!key_file_path(dir.path()).exists());

        // An actual rescue file survives a failed keychain write or a conflicting key.
        let original = load_from_file(dir.path(), true).unwrap();
        let rescued = resolve_keyring_key(dir.path(), Err(Error::NoEntry), false, |_| {
            Err(Error::NoDefaultStore)
        })
        .unwrap();
        assert!(rescued == original);
        let different = generate_key();
        let loaded = resolve_keyring_key(dir.path(), Ok(encode_key_b64(&different)), false, |_| {
            panic!("the keychain already has a key")
        })
        .unwrap();
        assert!(loaded == different);
        assert!(read_key_file(dir.path()).unwrap() == Some(original));
    }

    #[test]
    fn a_keyring_that_cannot_be_reached_fails_closed_without_a_legacy_key() {
        let dir = temp_dir("keyring-closed");
        let error = unavailable_keyring(&dir, false)
            .err()
            .expect("missing key must be reported");
        assert!(
            error.contains("DATAZEN_NATIVE_KEYRING=file"),
            "the sentence names the escape hatch: {error}"
        );
        assert!(!key_file_path(&dir).exists(), "no key file appeared");

        // The legacy .key file is the escape hatch: with one, the keyring
        // backend loads it.
        let key = load_or_create_master_key(&dir, KeyBackend::File).unwrap();
        let rescued = unavailable_keyring(&dir, false).unwrap();
        assert!(
            rescued == key,
            "the legacy key file answers when the keyring cannot"
        );
    }
}
