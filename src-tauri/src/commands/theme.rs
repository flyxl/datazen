//! Surface background IPC for webview boot cache.

use tauri::{AppHandle, Manager};

use super::error::CommandError;
use crate::theme::surface_bg::SurfaceBgCache;

#[tauri::command]
pub fn set_surface_background(app: AppHandle, hex: String, dark: bool) -> Result<(), CommandError> {
    let cache = app
        .try_state::<SurfaceBgCache>()
        .ok_or_else(|| CommandError::Internal("surface background cache missing".into()))?;
    cache.set(&hex, dark).map_err(CommandError::Validation)?;
    Ok(())
}
