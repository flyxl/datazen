//! Connection import path helpers + app-data archive + encryption key + restart.
//! Business boundary split from `config.rs` to keep each file under push limits.

use super::*;
use super::connection_import::{detect_import_path, parse_from_app, ImportApp, PathContext};
use crate::app_data_archive;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedConnectionImportPath {
    pub path: String,
    pub found: bool,
}

#[tauri::command]
pub fn detect_connection_import_path(
    source: String,
) -> Result<DetectedConnectionImportPath, CommandError> {
    let app = ImportApp::parse(&source)?;
    let detected = detect_import_path(app, &PathContext::from_env());
    Ok(DetectedConnectionImportPath {
        path: detected.path,
        found: detected.found,
    })
}

fn import_file_filters(app: ImportApp) -> (&'static str, &'static [&'static str]) {
    match app {
        ImportApp::Dbx => ("DBX", &["db", "json", "sqlite"]),
        ImportApp::Navicat => ("Navicat", &["ncx", "xml"]),
        ImportApp::DataGrip => ("DataGrip", &["xml"]),
        ImportApp::DBeaver => ("DBeaver", &["json"]),
        ImportApp::TablePlus => ("TablePlus", &["plist", "tableplusconnection"]),
    }
}

/// Native file or folder picker for competitor data/install paths. Path never crosses as a write.
#[tauri::command]
pub async fn pick_connection_import_path_with_dialog(
    app: AppHandle,
    mode: String,
    source: String,
) -> Result<Option<String>, CommandError> {
    let import_app = ImportApp::parse(&source)?;
    let is_folder = mode.trim().eq_ignore_ascii_case("folder");
    let picked = if is_folder {
        super::dialog::pick_folder(&app).await?
    } else {
        let (label, exts) = import_file_filters(import_app);
        super::dialog::open_file(
            &app,
            vec![(
                label.to_string(),
                exts.iter().map(|s| s.to_string()).collect(),
            )],
        )
        .await?
    };
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

/// Import connections discovered from a known competitor app install path.
#[tauri::command]
pub async fn import_connections_from_app(
    state: State<'_, AppState>,
    source: String,
    password: String,
    path: Option<String>,
) -> Result<ImportConnectionsResult, CommandError> {
    let app = ImportApp::parse(&source)?;
    let password = password.trim().to_string();
    let path_buf = path.map(PathBuf::from);
    let parsed = parse_from_app(
        app,
        path_buf.as_deref(),
        import_password_option(&password),
        &PathContext::from_env(),
    )?;
    let incoming = parsed.connections;
    let incoming_groups = parsed.groups;
    let skipped = parsed.skipped;
    let source_format = format_label(parsed.format).to_string();

    let result = apply_connection_import_impl(
        &state,
        incoming,
        incoming_groups,
        skipped,
        source_format.clone(),
    )
    .await?;

    tracing::info!(
        imported = result.imported,
        overwritten = result.overwritten,
        groups_added = result.groups_added,
        skipped = result.skipped.len(),
        %source_format,
        "import_connections_from_app OK"
    );
    Ok(result)
}

/// Export the app data ZIP archive to `dest` with options from settings.
async fn export_app_data_to_dest(state: &AppState, dest: PathBuf) -> Result<(), CommandError> {
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
async fn import_app_data_from_source(
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
/// Merges the former webdriver-only raw-path `export_app_data(path)` and
/// `export_app_data_with_dialog` into one IPC (decision 3): production callers
/// omit `override_path` and go through the dialog; E2E passes `override_path`.
#[tauri::command]
pub async fn export_app_data(
    app: AppHandle,
    state: State<'_, AppState>,
    default_file_name: String,
    override_path: Option<String>,
) -> Result<Option<()>, CommandError> {
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
        return Ok(None);
    };
    export_app_data_to_dest(&state, dest).await?;
    Ok(Some(()))
}

/// Native open dialog + app-data ZIP import.
///
/// Merges the former webdriver-only raw-path `import_app_data(path)` and
/// `import_app_data_with_dialog` into one IPC (decision 3): production callers
/// omit `override_path` and go through the dialog; E2E passes `override_path`.
///
/// When the archive contains a `.key` file and the target data dir already has
/// an encryption key, the existing key is preserved unless `replace_key` is true.
#[tauri::command]
pub async fn import_app_data(
    app: AppHandle,
    state: State<'_, AppState>,
    replace_key: bool,
    override_path: Option<String>,
) -> Result<Option<()>, CommandError> {
    let source = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => Some(path),
        None => {
            super::dialog::open_file(
                &app,
                vec![("DataZen Archive".into(), vec!["zip".into()])],
            )
            .await?
        }
    };
    let Some(source) = source else {
        return Ok(None);
    };
    let options = app_data_archive::ImportOptions {
        replace_encryption_key: replace_key,
    };
    import_app_data_from_source(&state, source, options).await?;
    Ok(Some(()))
}

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

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    tracing::info!("restart_app");
    app.restart();
}
