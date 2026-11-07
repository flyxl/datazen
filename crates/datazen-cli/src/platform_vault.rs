//! Read-only access to the desktop's platform vault.
//!
//! The desktop stores the AES master key in the OS platform vault on macOS and
//! Windows. The CLI must be able to read that same key when the user has not
//! opted into the legacy `.key` file; otherwise an encrypted profile would
//! appear to have a password but be unusable. This module only reads — the CLI
//! never creates or moves a key.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

const SERVICE: &str = "com.datazen.encryption-key";
const ACCOUNT: &str = "master-key";

pub fn available() -> bool {
    #[cfg(target_os = "macos")]
    {
        return std::path::Path::new("/usr/bin/security").exists();
    }
    #[cfg(target_os = "windows")]
    {
        return true;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// Whether the desktop's auto-detected backend is the platform vault.
pub fn preferred() -> bool {
    #[cfg(target_os = "windows")]
    {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        return available() && macos_prefers_platform_vault();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// The desktop chooses the platform vault only for unsigned/ad-hoc macOS
/// binaries; signed builds use the `keyring` crate. The CLI uses the same
/// signal so the doctor report and reads match the desktop.
pub fn macos_prefers_platform_vault() -> bool {
    #[cfg(target_os = "macos")]
    {
        let Ok(exe) = std::env::current_exe() else {
            return true;
        };
        let path = std::fs::canonicalize(&exe).unwrap_or(exe);
        let Ok(output) = std::process::Command::new("/usr/bin/codesign")
            .args(["-dv", "--verbose=4"])
            .arg(&path)
            .output()
        else {
            return true;
        };
        let text = String::from_utf8_lossy(&output.stderr);
        text.contains("code object is not signed")
            || text.contains("Signature=adhoc")
            || text.contains("flags=0x2(adhoc)")
            || text.contains("flags=0x2 (adhoc)")
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

pub fn load() -> Result<Option<[u8; 32]>, String> {
    #[cfg(target_os = "macos")]
    {
        return macos::load();
    }
    #[cfg(target_os = "windows")]
    {
        return windows::load();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(None)
    }
}

#[allow(dead_code)]
fn decode(encoded: &str) -> Result<[u8; 32], String> {
    let bytes = BASE64
        .decode(encoded.trim())
        .map_err(|_| "the platform vault key is not valid base64".to_string())?;
    bytes
        .try_into()
        .map_err(|_| "the platform vault key has an invalid length".to_string())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::process::Command;

    pub fn load() -> Result<Option<[u8; 32]>, String> {
        let output = Command::new("/usr/bin/security")
            .args(["find-generic-password", "-a", ACCOUNT, "-s", SERVICE, "-w"])
            .output()
            .map_err(|error| format!("security CLI failed: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("SecKeychainSearchCopyNext") || stderr.contains("not found") {
                return Ok(None);
            }
            return Err(format!("security find-generic-password failed: {stderr}"));
        }
        let encoded = String::from_utf8_lossy(&output.stdout);
        decode(&encoded).map(Some)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::ffi::c_void;
    use std::path::PathBuf;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    extern "system" {
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

    fn dpapi_path() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("datazen")
            .join(".key.dpapi")
    }

    pub fn load() -> Result<Option<[u8; 32]>, String> {
        let path = dpapi_path();
        let Ok(encrypted) = std::fs::read(&path) else {
            return Ok(None);
        };
        let plaintext = unprotect(&encrypted)?;
        decode(&String::from_utf8_lossy(&plaintext)).map(Some)
    }

    fn unprotect(encrypted: &[u8]) -> Result<Vec<u8>, String> {
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
                return Err("CryptUnprotectData failed".into());
            }
            let blob =
                std::slice::from_raw_parts(data_out.pb_data, data_out.cb_data as usize).to_vec();
            LocalFree(data_out.pb_data as *mut c_void);
            Ok(blob)
        }
    }
}
