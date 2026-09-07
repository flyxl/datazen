use super::error::CommandError;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

#[tauri::command]
pub async fn write_clipboard(app: AppHandle, text: String) -> Result<(), CommandError> {
    app.clipboard()
        .write_text(text)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn read_clipboard(app: AppHandle) -> Result<String, CommandError> {
    app.clipboard()
        .read_text()
        .map_err(|e| CommandError::Internal(e.to_string()))
}
