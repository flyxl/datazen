//! App-data ZIP archive: export and import the whole `{appData}` directory.
//!
//! Split out of the former `config_import_and_archive.rs`, which bundled this
//! with connection import, the encryption key and app restart — four unrelated
//! concerns in one file.

use super::error::{resolve_override_path, CmdExt, CommandError, OVERRIDE_DISABLED_MSG};
use super::AppState;
use crate::app_data_archive;
use crate::store::AppSettings;
use std::path::PathBuf;
use tauri::{AppHandle, State};

pub(crate) fn export_options_from_settings(
    settings: &AppSettings,
) -> app_data_archive::ExportOptions {
    app_data_archive::ExportOptions {
        include_dashboard_runs: settings.monitor.export_include_dashboard_runs,
    }
}

/// Export the app data ZIP archive to `dest` with options from settings.
pub(super) async fn export_app_data_to_dest(
    state: &AppState,
    dest: PathBuf,
) -> Result<(), CommandError> {
    let settings = state.store.get_settings().await;
    let data_dir = state.store.data_dir().clone();
    let options = export_options_from_settings(&settings);
    tokio::task::spawn_blocking(move || {
        app_data_archive::export_app_data_with_options(&data_dir, &dest, options)
    })
    .await
    .map_err(|e| CommandError::Internal(format!("export_app_data task: {e}")))?
    .cmd_err("export_app_data")?;
    tracing::info!("export_app_data OK");
    Ok(())
}

/// Import an app data ZIP archive from `source`.
pub(super) async fn import_app_data_from_source(
    state: &AppState,
    source: PathBuf,
    options: app_data_archive::ImportOptions,
) -> Result<(), CommandError> {
    let data_dir = state.store.data_dir().clone();
    tokio::task::spawn_blocking(move || {
        app_data_archive::import_app_data_with_options(&data_dir, &source, options)
    })
    .await
    .map_err(|e| CommandError::Internal(format!("import_app_data task: {e}")))?
    .cmd_err("import_app_data")?;
    tracing::info!("import_app_data OK");
    Ok(())
}

/// Native save dialog + app-data ZIP export.
///
/// `override_path` is the webdriver/E2E escape hatch; production callers omit
/// it and go through the dialog. Returns `true` if saved, `false` if the user
/// dismissed the dialog — `ConnectionPage` branches on that, so this must stay
/// a plain `bool` and not `Option<()>` (both of which would serialise to
/// `null` and make a successful export look cancelled).
#[tauri::command]
pub async fn export_app_data(
    app: AppHandle,
    state: State<'_, AppState>,
    default_file_name: String,
    override_path: Option<String>,
) -> Result<bool, CommandError> {
    let dest = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => Some(path),
        None => {
            super::dialog::save_file(
                &app,
                ("DataZen Archive".into(), vec!["zip".into()]),
                default_file_name,
            )
            .await?
        }
    };
    let Some(dest) = dest else {
        return Ok(false); // user dismissed the dialog
    };
    export_app_data_to_dest(&state, dest).await?;
    Ok(true)
}

/// Open the native file picker for app-data ZIP import (file only; no import).
#[tauri::command]
pub async fn pick_app_data_import_file(app: AppHandle) -> Result<Option<String>, CommandError> {
    let picked = super::dialog::open_file(&app, vec![("ZIP".into(), vec!["zip".into()])]).await?;
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

/// ZIP import from a path chosen in the UI (TablePlus-style: pick file first,
/// confirm in the webview, then import). Returns `true` if imported.
///
/// Production callers pass `source_path` from [`pick_app_data_import_file`]
/// after the web confirm; E2E passes `override_path`, which requires a
/// webdriver build and skips the picker.
///
/// When the archive contains a `.key` file and the target data dir already has
/// an encryption key, the import is rejected unless `allow_key_overwrite` is
/// `true`. This protects against silent key replacement that could render
/// encrypted data unrecoverable.
#[tauri::command]
pub async fn import_app_data(
    state: State<'_, AppState>,
    source_path: Option<String>,
    override_path: Option<String>,
    allow_key_overwrite: Option<bool>,
) -> Result<bool, CommandError> {
    let source = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => path,
        None => {
            let Some(path_str) = source_path else {
                return Err(CommandError::Validation(
                    "No app data import file selected".into(),
                ));
            };
            PathBuf::from(path_str)
        }
    };

    let options = app_data_archive::ImportOptions {
        allow_key_overwrite: allow_key_overwrite.unwrap_or(false),
    };
    import_app_data_from_source(&state, source, options).await?;
    Ok(true)
}

#[cfg(test)]
#[path = "app_archive_tests.rs"]
mod tests;
