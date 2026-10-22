//! Master-key backend selection — a **startup-time fact**.
//!
//! `DATAZEN_KEYRING` decides where the AES master key lives. It is read
//! **once**, during startup assembly ([`resolve_key_backend`]), and the
//! resolved [`KeyBackend`] is then threaded explicitly through the call chain.
//! No other code path re-reads the process environment to pick a backend.
//!
//! Why that matters: `std::env::set_var` is **process-global**. A thread that
//! mutates `DATAZEN_KEYRING` used to be able to flip the backend underneath a
//! concurrent caller, because the backend was re-read on every call. On macOS
//! the `Keyring` backend opens Keychain Services, which can raise a modal
//! authorization dialog and hang the whole process. With the backend resolved
//! once and passed down, no env write can change the backend of a running
//! store.
//!
//! Selection order (unchanged):
//! 1. `DATAZEN_KEYRING=file` → [`KeyBackend::File`]
//! 2. `DATAZEN_KEYRING=keyring` → [`KeyBackend::Keyring`]
//! 3. unset → auto-detect:
//!    - macOS unsigned/adhoc → [`KeyBackend::PlatformVault`] (security CLI → login keychain)
//!    - Windows → [`KeyBackend::PlatformVault`] (DPAPI)
//!    - macOS signed / Linux → [`KeyBackend::Keyring`]
//!
//! Under `cfg(test)` the OS keychain is reachable only when the run opted in
//! with `DATAZEN_TEST_KEYRING=1`; see [`test_keyring_opted_in`].

/// Where the AES master key is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBackend {
    /// Plaintext `.key` file (explicit opt-in via `DATAZEN_KEYRING=file`).
    File,
    /// OS keychain via `keyring` crate (macOS signed, Linux).
    Keyring,
    /// Platform vault: macOS security CLI / Windows DPAPI.
    PlatformVault,
}

/// Read `DATAZEN_KEYRING` **once** and resolve the backend.
///
/// Call this from startup assembly only (GUI: `DesktopRepository::init`,
/// headless MCP: `bootstrap::run::run_mcp_stdio`); pass the result down
/// explicitly — see [`crate::store::key_store::load_or_create_master_key`].
pub fn resolve_key_backend() -> KeyBackend {
    let value = std::env::var("DATAZEN_KEYRING").ok();
    #[cfg(test)]
    let keyring_allowed = test_keyring_opted_in();
    // Outside test builds the OS keychain is a supported backend, so the
    // `keyring` value always selects it.
    #[cfg(not(test))]
    let keyring_allowed = true;
    backend_from_env_value(value.as_deref(), keyring_allowed)
}

/// Pure mapping from a `DATAZEN_KEYRING` value to a backend.
///
/// `keyring_allowed` is the test-build gate: under `cfg(test)` the OS keychain
/// is selected only when the run explicitly opted in. Keeping the mapping pure
/// lets the rules be pinned by unit tests without touching the environment.
fn backend_from_env_value(value: Option<&str>, keyring_allowed: bool) -> KeyBackend {
    match value {
        Some("file") => KeyBackend::File,
        Some("keyring") => {
            if keyring_allowed {
                KeyBackend::Keyring
            } else {
                KeyBackend::File
            }
        }
        _ => default_backend(),
    }
}

/// Backend for an unset or unrecognized `DATAZEN_KEYRING`.
fn default_backend() -> KeyBackend {
    // Never reach for the OS keychain from a unit test: a Keychain Services
    // call made inside a login session can block on a system authorization
    // dialog ("找不到用于储存 "app-encryption-key" 的钥匙串 "login""), which
    // hangs the whole `cargo test` run.
    #[cfg(test)]
    {
        KeyBackend::File
    }
    #[cfg(not(test))]
    {
        auto_detect_backend()
    }
}

/// Whether this **test run** is allowed to touch the OS keychain.
///
/// macOS raises a modal "找不到用于储存 … 的钥匙串 …" dialog for Keychain Services
/// calls made inside a login session when the keychain search list is broken, and
/// that call blocks until a human clicks it. Gate steps (`pnpm ci:local`,
/// `cargo test -p datazen --lib`) must never depend on GUI interaction, so keychain
/// coverage is opt-in via `DATAZEN_TEST_KEYRING=1`. Production code paths are
/// unaffected — this only gates `cfg(test)` behaviour.
#[cfg(test)]
pub(crate) fn test_keyring_opted_in() -> bool {
    std::env::var("DATAZEN_TEST_KEYRING").ok().as_deref() == Some("1")
}

#[cfg(not(test))]
fn auto_detect_backend() -> KeyBackend {
    // macOS: unsigned/adhoc → platform vault; signed → keyring
    #[cfg(target_os = "macos")]
    {
        if super::platform_vault::platform_vault_available()
            && macos_codesign_is_adhoc_or_unsigned()
        {
            tracing::info!(
                "Using platform vault (macOS security CLI) for encryption key; \
                 set DATAZEN_KEYRING=file for file backend, or DATAZEN_KEYRING=keyring for keyring crate"
            );
            return KeyBackend::PlatformVault;
        }
        KeyBackend::Keyring
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
        KeyBackend::Keyring
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
    let text = format!("{}", String::from_utf8_lossy(&out.stderr),);
    if text.contains("code object is not signed") {
        return true;
    }
    text.contains("Signature=adhoc")
        || text.contains("flags=0x2(adhoc)")
        || text.contains("flags=0x2 (adhoc)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_file_forces_file_backend() {
        assert_eq!(
            backend_from_env_value(Some("file"), false),
            KeyBackend::File
        );
        assert_eq!(backend_from_env_value(Some("file"), true), KeyBackend::File);
    }

    #[test]
    fn env_keyring_resolves_to_keyring_only_when_opted_in() {
        // Default: a stray `keyring` in the environment must not send the suite
        // into Keychain Services (that is what hangs `cargo test` on a macOS
        // dialog).
        assert_eq!(
            backend_from_env_value(Some("keyring"), false),
            KeyBackend::File
        );
        // Opt-in: the same value now means the real keyring backend.
        assert_eq!(
            backend_from_env_value(Some("keyring"), true),
            KeyBackend::Keyring
        );
    }

    #[test]
    fn key_backend_unset_uses_file_or_keyring_by_platform() {
        assert_eq!(backend_from_env_value(None, false), KeyBackend::File);
        assert_eq!(
            backend_from_env_value(Some("bogus"), false),
            KeyBackend::File
        );
    }

    /// The resolver must agree with the environment it is started in.
    ///
    /// This is a **reader**: it takes the process-wide env lock so it cannot
    /// observe a half-written value while an `#[ignore]`d keychain test is
    /// mutating the environment.
    #[test]
    fn resolve_key_backend_follows_the_process_environment() {
        let _guard = crate::testing::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let value = std::env::var("DATAZEN_KEYRING").ok();
        let expected = match value.as_deref() {
            Some("file") => KeyBackend::File,
            Some("keyring") => {
                if test_keyring_opted_in() {
                    KeyBackend::Keyring
                } else {
                    KeyBackend::File
                }
            }
            _ => KeyBackend::File,
        };
        assert_eq!(resolve_key_backend(), expected);
    }

    #[test]
    fn macos_codesign_detect_runs_without_panic() {
        #[cfg(target_os = "macos")]
        {
            let _ = macos_codesign_is_adhoc_or_unsigned();
        }
    }

    /// The platform vault probe decides the auto-detected backend, so its
    /// answer must match the platform it runs on.
    #[test]
    fn platform_vault_probe_matches_the_platform() {
        let available = super::super::platform_vault::platform_vault_available();
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        assert!(available, "macOS/Windows always have a platform vault");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        assert!(!available, "Linux has no platform vault");
    }
}
