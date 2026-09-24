//! Platform-specific encryption key vault.
//!
//! Provides secure key storage for each platform:
//! - **macOS (unsigned/adhoc):** `security` CLI → login keychain (ACL-bound to `/usr/bin/security`)
//! - **Windows:** DPAPI (`CryptProtectData`) → user-scoped encrypted blob
//! - **Linux / macOS (signed):** falls back to `keyring` crate (not handled here)
//!
//! `DATAZEN_KEYRING=file` bypasses all of this and uses a plaintext `.key` file.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use super::StoreError;

const VAULT_SERVICE: &str = "com.datazen.encryption-key";
const VAULT_ACCOUNT: &str = "master-key";

// ---------------------------------------------------------------------------
// macOS — security CLI
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub mod macos_security {
    use super::*;

    /// Store key in macOS login keychain via `security` CLI.
    /// ACL binds to `/usr/bin/security` — survives ad-hoc re-signing.
    pub fn store(key: &[u8; 32]) -> Result<(), StoreError> {
        let b64 = BASE64.encode(key);
        // -U: update if already exists; -T: allow /usr/bin/security to access
        let status = std::process::Command::new("/usr/bin/security")
            .args([
                "add-generic-password",
                "-a", VAULT_ACCOUNT,
                "-s", VAULT_SERVICE,
                "-w", &b64,
                "-U",
                "-T", "/usr/bin/security",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()
            .map_err(|e| StoreError::EncryptionError(format!("security CLI failed: {e}")))?;

        if status.success() {
            Ok(())
        } else {
            Err(StoreError::EncryptionError(format!(
                "security add-generic-password exited with {status}"
            )))
        }
    }

    /// Read key from macOS login keychain via `security` CLI.
    pub fn load() -> Result<Option<[u8; 32]>, StoreError> {
        let output = std::process::Command::new("/usr/bin/security")
            .args([
                "find-generic-password",
                "-a", VAULT_ACCOUNT,
                "-s", VAULT_SERVICE,
                "-w", // output only the password
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| StoreError::EncryptionError(format!("security CLI failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("SecKeychainSearchCopyNext") || stderr.contains("not found") {
                return Ok(None);
            }
            return Err(StoreError::EncryptionError(format!(
                "security find-generic-password failed: {stderr}"
            )));
        }

        let b64 = String::from_utf8_lossy(&output.stdout);
        let key = decode_key_b64(&b64)?;
        Ok(Some(key))
    }

    /// Delete key from macOS login keychain via `security` CLI.
    pub fn delete() -> Result<(), StoreError> {
        let _ = std::process::Command::new("/usr/bin/security")
            .args([
                "delete-generic-password",
                "-a", VAULT_ACCOUNT,
                "-s", VAULT_SERVICE,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Windows — DPAPI (raw FFI, no windows crate dependency)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub mod windows_dpapi {
    use super::*;
    use std::ffi::c_void;

    // Raw FFI for DPAPI — avoids windows crate version friction.
    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    extern "system" {
        fn CryptProtectData(
            pdata_in: *const DataBlob,
            sz_data_desc: *const u16,
            p_optional_ent: *const DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *mut c_void,
            dw_flags: u32,
            pdata_out: *mut DataBlob,
        ) -> i32;

        fn CryptUnprotectData(
            pdata_in: *const DataBlob,
            ppsz_data_desc: *mut *mut u16,
            p_optional_ent: *const DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *mut c_void,
            dw_flags: u32,
            pdata_out: *mut DataBlob,
        ) -> i32;

        fn LocalFree(p_hmem: *mut c_void) -> *mut c_void;
    }

    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;
    const CRYPTPROTECT_LOCAL_MACHINE: u32 = 0x4;

    fn dpapi_path() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("datazen")
            .join(".key.dpapi")
    }

    fn protect(plaintext: &[u8]) -> Result<Vec<u8>, StoreError> {
        unsafe {
            let mut data_in = DataBlob {
                cb_data: plaintext.len() as u32,
                pb_data: plaintext.as_ptr() as *mut u8,
            };
            let mut data_out = DataBlob {
                cb_data: 0,
                pb_data: std::ptr::null_mut(),
            };

            let ok = CryptProtectData(
                &data_in,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_LOCAL_MACHINE | CRYPTPROTECT_UI_FORBIDDEN,
                &mut data_out,
            );
            if ok == 0 {
                return Err(StoreError::EncryptionError(
                    "CryptProtectData failed".into(),
                ));
            }
            let blob =
                std::slice::from_raw_parts(data_out.pb_data, data_out.cb_data as usize).to_vec();
            LocalFree(data_out.pb_data as *mut c_void);
            Ok(blob)
        }
    }

    fn unprotect(encrypted: &[u8]) -> Result<Vec<u8>, StoreError> {
        unsafe {
            let mut data_in = DataBlob {
                cb_data: encrypted.len() as u32,
                pb_data: encrypted.as_ptr() as *mut u8,
            };
            let mut data_out = DataBlob {
                cb_data: 0,
                pb_data: std::ptr::null_mut(),
            };

            let ok = CryptUnprotectData(
                &data_in,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut data_out,
            );
            if ok == 0 {
                return Err(StoreError::EncryptionError(
                    "CryptUnprotectData failed".into(),
                ));
            }
            let blob =
                std::slice::from_raw_parts(data_out.pb_data, data_out.cb_data as usize).to_vec();
            LocalFree(data_out.pb_data as *mut c_void);
            Ok(blob)
        }
    }

    pub fn store(key: &[u8; 32]) -> Result<(), StoreError> {
        let path = dpapi_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        }
        let b64 = BASE64.encode(key);
        let protected = protect(b64.as_bytes())?;
        std::fs::write(&path, &protected)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        Ok(())
    }

    pub fn load() -> Result<Option<[u8; 32]>, StoreError> {
        let path = dpapi_path();
        let Ok(encrypted) = std::fs::read(&path) else {
            return Ok(None);
        };
        let plaintext = unprotect(&encrypted)?;
        let b64 = String::from_utf8_lossy(&plaintext);
        let key = decode_key_b64(&b64)?;
        Ok(Some(key))
    }

    pub fn delete() -> Result<(), StoreError> {
        let path = dpapi_path();
        let _ = std::fs::remove_file(&path);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Dispatch helpers — platform-agnostic public API
// ---------------------------------------------------------------------------

fn decode_key_b64(key_b64: &str) -> Result<[u8; 32], StoreError> {
    let key_bytes = BASE64
        .decode(key_b64.trim())
        .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
    if key_bytes.len() != 32 {
        return Err(StoreError::EncryptionError(
            "Invalid key length".into(),
        ));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);
    Ok(key)
}

/// Whether the platform vault (security CLI / DPAPI) is available in this environment.
pub fn platform_vault_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::path::Path::new("/usr/bin/security").exists()
    }
    #[cfg(target_os = "windows")]
    {
        true // DPAPI is always available on Windows
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// Try to load key from platform vault.
pub fn load_from_vault() -> Result<Option<[u8; 32]>, StoreError> {
    #[cfg(target_os = "macos")]
    {
        macos_security::load()
    }
    #[cfg(target_os = "windows")]
    {
        windows_dpapi::load()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(None)
    }
}

/// Store key to platform vault.
pub fn store_to_vault(key: &[u8; 32]) -> Result<(), StoreError> {
    #[cfg(target_os = "macos")]
    {
        macos_security::store(key)
    }
    #[cfg(target_os = "windows")]
    {
        windows_dpapi::store(key)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(StoreError::EncryptionError(
            "No platform vault available on this OS".into(),
        ))
    }
}

/// Delete key from platform vault (best-effort).
pub fn delete_from_vault() -> Result<(), StoreError> {
    #[cfg(target_os = "macos")]
    {
        macos_security::delete()
    }
    #[cfg(target_os = "windows")]
    {
        windows_dpapi::delete()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(())
    }
}
