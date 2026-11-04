//! Export of the app encryption key material.

use super::error::{CmdExt, CommandError};
use super::AppState;
use tauri::{AppHandle, State};

/// Bytes written when exporting the encryption key file (base64 text, trimmed).
pub fn encryption_key_export_bytes(key_b64: &str) -> Vec<u8> {
    key_b64.trim().as_bytes().to_vec()
}

/// Native save dialog for the app encryption key (`.key` material). Path never crosses the webview.
#[tauri::command]
pub async fn save_encryption_key_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    default_file_name: String,
) -> Result<bool, CommandError> {
    let key_b64 = state.store.encryption_key_b64();
    let bytes = encryption_key_export_bytes(&key_b64);

    let picked = super::dialog::save_file(
        &app,
        ("Encryption Key".into(), vec!["key".into()]),
        default_file_name,
    )
    .await?;
    let Some(dest) = picked else {
        return Ok(false);
    };
    tokio::fs::write(&dest, &bytes)
        .await
        .cmd_err("save_encryption_key_with_dialog")?;
    Ok(true)
}

#[cfg(test)]
#[path = "encryption_key_tests.rs"]
mod tests;
