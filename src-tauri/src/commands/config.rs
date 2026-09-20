use super::connection_import::{
    self, detect_import_path, format_label, parse_from_app, parse_import_file, ImportApp,
    PathContext,
};
use super::error::{resolve_override_path, CmdExt, CommandError, OVERRIDE_DISABLED_MSG};
use super::AppState;
use crate::app_data_archive;
use crate::db::ConnectionConfig;
use crate::i18n_locale;
use crate::store::AppSettings;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use tauri::{AppHandle, State};

pub(crate) async fn get_groups_impl(state: &AppState) -> Result<Vec<String>, CommandError> {
    Ok(state.store.get_groups().await)
}

pub(crate) async fn save_groups_impl(
    state: &AppState,
    groups: Vec<String>,
) -> Result<(), CommandError> {
    tracing::info!(count = groups.len(), "save_groups");
    state.store.save_groups(groups).await.cmd_err("save_groups")
}

pub(crate) async fn get_settings_impl(state: &AppState) -> Result<AppSettings, CommandError> {
    Ok(state.store.get_settings().await)
}

pub(crate) async fn save_settings_impl(
    state: &AppState,
    mut settings: AppSettings,
) -> Result<(), CommandError> {
    settings.connection_pool_size =
        crate::store::clamp_connection_pool_size(settings.connection_pool_size);
    tracing::debug!(theme_mode = %settings.theme.mode, "save_settings");
    state
        .store
        .save_settings(settings.clone())
        .await
        .cmd_err("save_settings")?;
    crate::redis_flush_gate::sync_from_settings(&settings);
    state
        .monitor_engine
        .reload_from_store()
        .await
        .map_err(|e| {
            CommandError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })
        .cmd_err("save_settings")?;
    if let Some(app) = state.monitor_engine.app_handle() {
        crate::tray::sync_tray_async(&app).await;
    }
    Ok(())
}

pub(crate) async fn get_log_path_impl(state: &AppState) -> Result<String, CommandError> {
    let settings = state.store.get_settings().await;
    let data_dir = state.store.data_dir();
    let log_dir = crate::resolve_log_dir(data_dir, &settings.log_path);
    Ok(log_dir.to_string_lossy().to_string())
}

pub(crate) fn get_app_executable_path_impl() -> Result<String, CommandError> {
    let exe = std::env::current_exe().cmd_err("get_app_executable_path")?;
    let path = std::fs::canonicalize(&exe).unwrap_or(exe);
    let path_str = path.to_string_lossy().to_string();
    #[cfg(windows)]
    let path_str = path_str
        .strip_prefix(r"\\?\")
        .unwrap_or(&path_str)
        .to_string();
    Ok(path_str)
}

#[tauri::command]
pub async fn get_groups(state: State<'_, AppState>) -> Result<Vec<String>, CommandError> {
    get_groups_impl(&state).await
}

#[tauri::command]
pub async fn save_groups(
    state: State<'_, AppState>,
    groups: Vec<String>,
) -> Result<(), CommandError> {
    save_groups_impl(&state, groups).await
}
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, CommandError> {
    get_settings_impl(&state).await
}

#[tauri::command]
pub fn get_system_ui_language() -> String {
    i18n_locale::default_ui_language()
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<(), CommandError> {
    save_settings_impl(&state, settings).await
}

#[tauri::command]
pub async fn get_log_path(state: State<'_, AppState>) -> Result<String, CommandError> {
    get_log_path_impl(&state).await
}

#[tauri::command]
pub fn get_app_executable_path() -> Result<String, CommandError> {
    get_app_executable_path_impl()
}

fn path_is_under(child: &std::path::Path, root: &std::path::Path) -> bool {
    child.starts_with(root)
}

/// Legacy path-based IPC is only available in webdriver/E2E builds.
fn require_webdriver_path_ipc(disabled_msg: &'static str) -> Result<(), CommandError> {
    super::error::require_webdriver_path_ipc(disabled_msg)
}

/// Open the application log directory (path resolved server-side).
#[tauri::command]
pub async fn open_log_dir(state: State<'_, AppState>) -> Result<(), CommandError> {
    let settings = state.store.get_settings().await;
    let data_dir = state.store.data_dir();
    let log_dir = crate::resolve_log_dir(data_dir, &settings.log_path);
    std::fs::create_dir_all(&log_dir).map_err(CommandError::from)?;
    open::that(&log_dir).map_err(|e| CommandError::Internal(format!("open_log_dir: {e}")))
}

/// Open the workflows directory (path resolved server-side).
#[tauri::command]
pub async fn open_workflows_dir(state: State<'_, AppState>) -> Result<(), CommandError> {
    let dir = state.workflow_registry.workflows_dir().clone();
    std::fs::create_dir_all(&dir).map_err(CommandError::from)?;
    open::that(&dir).map_err(|e| CommandError::Internal(format!("open_workflows_dir: {e}")))
}

/// Open the configured AI context directory (path resolved server-side).
#[tauri::command]
pub async fn open_context_dir(state: State<'_, AppState>) -> Result<(), CommandError> {
    let settings = state.store.get_settings().await;
    let data_dir = state.store.data_dir();
    let context_dir = crate::resolve_context_dir(data_dir, &settings.context_dir);
    std::fs::create_dir_all(&context_dir).map_err(CommandError::from)?;
    open::that(&context_dir).map_err(|e| CommandError::Internal(format!("open_context_dir: {e}")))
}

/// Open a path only if it lies under the app data dir or configured context dir.
/// Prefer open_log_dir / open_workflows_dir / open_context_dir when possible.
pub(crate) async fn open_path_impl(state: &AppState, path: String) -> Result<(), CommandError> {
    if path.starts_with("http://") || path.starts_with("https://") {
        return open::that(&path).map_err(|e| CommandError::Internal(format!("open_path: {e}")));
    }

    require_webdriver_path_ipc(
        "open_path disabled; use open_log_dir / open_workflows_dir / open_context_dir",
    )?;
    let requested = PathBuf::from(&path);
    if requested.to_string_lossy().contains("..") {
        return Err(CommandError::Validation(
            "Path traversal not allowed".into(),
        ));
    }

    let data_dir = state.store.data_dir().clone();
    if !requested.exists() && path_is_under(&requested, &data_dir) {
        std::fs::create_dir_all(&requested).map_err(CommandError::from)?;
    }

    let canonical = requested
        .canonicalize()
        .map_err(|e| CommandError::Validation(format!("Cannot resolve path: {e}")))?;
    let data_canon = data_dir.canonicalize().unwrap_or(data_dir.clone());

    let settings = state.store.get_settings().await;
    let context_root = crate::resolve_context_dir(&data_dir, &settings.context_dir);
    let context_canon = context_root.canonicalize().ok();

    let allowed = path_is_under(&canonical, &data_canon)
        || context_canon
            .as_ref()
            .is_some_and(|c| path_is_under(&canonical, c));
    if !allowed {
        return Err(CommandError::Validation(
            "open_path only allows app data or context directories".into(),
        ));
    }

    open::that(&canonical).map_err(|e| CommandError::Internal(format!("open_path: {e}")))
}

#[tauri::command]
pub async fn open_path(state: State<'_, AppState>, path: String) -> Result<(), CommandError> {
    open_path_impl(&state, path).await
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportConnectionsResult {
    pub imported: u32,
    pub overwritten: u32,
    pub groups_added: u32,
    #[serde(default)]
    pub skipped: Vec<String>,
    #[serde(default)]
    pub source_format: String,
}

fn validate_share_password(password: &str) -> Result<(), CommandError> {
    if password.trim().is_empty() {
        return Err(CommandError::Validation("Password is required".into());
    }
    Ok(())
}

/// Trim export/import share passwords so encrypt and decrypt use the same bytes.
fn normalize_share_password(password: String) -> Result<String, CommandError> {
    validate_share_password(&password)?;
    Ok(password.trim().to_string())
}

fn merge_connection_import_stats(
    existing_ids: &HashSet<String>,
    incoming: &[ConnectionConfig],
) -> (u32, u32) {
    let mut imported = 0u32;
    let mut overwritten = 0u32;
    for conn in incoming {
        if existing_ids.contains(&conn.id) {
            overwritten += 1;
        } else {
            imported += 1;
        }
    }
    (imported, overwritten)
}

fn merge_group_lists(existing: &[String], incoming: &[String]) -> (Vec<String>, u32) {
    let before = existing.len();
    let mut set: BTreeSet<String> = existing.iter().cloned().collect();
    for g in incoming {
        set.insert(g.clone());
    }
    let groups_added = set.len().saturating_sub(before) as u32;
    (set.into_iter().collect(), groups_added)
}

fn build_encrypted_connections_export(
    connections: &[ConnectionConfig],
    _groups: &[String],
    password: &str,
) -> Result<Vec<u8>, CommandError> {
    validate_share_password(password)?;
    connection_import::build_tableplus_export(connections, password)
}

/// Build the encrypted `.datazenconnection` payload and write it to `dest`.
async fn write_connections_export(
    state: &AppState,
    password: &str,
    dest: PathBuf,
) -> Result<u32, CommandError> {
    let connections = state.store.get_connections().await;
    let groups = state.store.get_groups().await;
    let count = connections.len() as u32;

    let bytes = build_encrypted_connections_export(&connections, &groups, password)?;

    tokio::fs::write(&dest, &bytes)
        .await
        .cmd_err("export_connections")?;

    tracing::info!(path = %dest.display(), count, "export_connections OK");
    Ok(count)
}

/// Open-dialog filters shared by [`import_connections_preview`] and
/// [`import_connections_with_dialog`] (pure data — the invocation itself lives
/// in the central [`super::dialog`] gateway).
fn connections_open_filters() -> Vec<(String, Vec<String>)> {
    vec![
        (
            "Connections".into(),
            vec![
                "json".into(),
                "xml".into(),
                "ncx".into(),
                "datazenconnection".into(),
                "tableplusconnection".into(),
            ],
        ),
        (
            "DataZen".into(),
            vec!["datazenconnection".into(), "json".into()],
        ),
        ("DataGrip XML".into(), vec!["xml".into()]),
        ("Navicat NCX".into(), vec!["ncx".into(), "xml".into()]),
        ("DBeaver JSON".into(), vec!["json".into()]),
        ("TablePlus".into(), vec!["tableplusconnection".into()]),
    ]
}

/// Native save dialog + RNCryptor `.datazenconnection` export.
///
/// Merges the former webdriver-only raw-path `export_connections(path)` and
/// `export_connections_with_dialog` into one IPC (decision 3): production
/// callers omit `override_path` and go through the dialog; E2E passes
/// `override_path`, which requires a webdriver build (`cfg!(feature =
/// "webdriver")`, see [`super::error::resolve_override_path`]). Returns the
/// connection count if saved, `None` if cancelled.
#[tauri::command]
pub async fn export_connections(
    app: AppHandle,
    state: State<'_, AppState>,
    password: String,
    default_file_name: String,
    override_path: Option<String>,
) -> Result<Option<u32>, CommandError> {
    let password = normalize_share_password(password)?;

    let dest = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => Some(path),
        None => {
            super::dialog::save_file(
                &app,
                ("DataZen".into(), vec!["datazenconnection".into()]),
                default_file_name,
            )
            .await?
        }
    };
    let Some(dest) = dest else {
        return Ok(None); // user dismissed the dialog
    };

    Ok(Some(
        write_connections_export(&state, &password, dest).await?,
    ))
}

fn import_password_option(password: &str) -> Option<&str> {
    if password.trim().is_empty() {
        None
    } else {
        Some(password)
    }
}

fn build_import_preview_json(parsed: &connection_import::ParsedImport) -> serde_json::Value {
    serde_json::json!({
        "connections": parsed.connections,
        "groups": parsed.groups,
        "skipped": parsed.skipped,
        "sourceFormat": format_label(parsed.format),
    })
}

pub(crate) fn export_options_from_settings(
    settings: &AppSettings,
) -> app_data_archive::ExportOptions {
    app_data_archive::ExportOptions {
        include_dashboard_runs: settings.monitor.export_include_dashboard_runs,
    }
}

pub(crate) async fn apply_connection_import_impl(
    state: &AppState,
    incoming: Vec<ConnectionConfig>,
    incoming_groups: Vec<String>,
    skipped: Vec<String>,
    source_format: String,
) -> Result<ImportConnectionsResult, CommandError> {
    let existing = state.store.get_connections().await;
    let existing_ids: HashSet<String> = existing.iter().map(|c| c.id.clone()).collect();
    let (imported, overwritten) = merge_connection_import_stats(&existing_ids, &incoming);

    for conn in incoming {
        state.store.save_connection(conn).await?;
    }

    let existing_groups = state.store.get_groups().await;
    let (merged_groups, groups_added) = merge_group_lists(&existing_groups, &incoming_groups);
    state.store.save_groups(merged_groups).await?;

    Ok(ImportConnectionsResult {
        imported,
        overwritten,
        groups_added,
        skipped,
        source_format,
    })
}

/// Parse an imported connections file into a preview payload (no store writes).
async fn build_import_preview_from_path(
    password: &str,
    source: PathBuf,
) -> Result<serde_json::Value, CommandError> {
    let bytes = tokio::fs::read(&source)
        .await
        .cmd_err("import_connections_preview")?;
    let parsed = parse_import_file(&source, &bytes, import_password_option(password))?;
    Ok(build_import_preview_json(&parsed))
}

/// Preview the contents of a connections file without importing.
///
/// Decision 3 form: production callers omit `override_path` and pick the file
/// through the native open dialog; E2E passes `override_path`, which requires
/// a webdriver build. Returns the preview JSON, or `None` when the dialog was
/// dismissed.
#[tauri::command]
pub async fn import_connections_preview(
    app: AppHandle,
    password: String,
    override_path: Option<String>,
) -> Result<Option<serde_json::Value>, CommandError> {
    let password = password.trim().to_string();
    let source = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => Some(path),
        None => super::dialog::open_file(&app, connections_open_filters()).await?,
    };
    let Some(source) = source else {
        return Ok(None); // user dismissed the dialog
    };
    tracing::info!(path = %source.display(), "import_connections_preview");
    Ok(Some(
        build_import_preview_from_path(&password, source).await?,
    ))
}

async fn import_connections_from_path(
    state: &AppState,
    password: &str,
    source: PathBuf,
) -> Result<ImportConnectionsResult, CommandError> {
    let bytes = tokio::fs::read(&source)
        .await
        .cmd_err("import_connections")?;

    let parsed = parse_import_file(&source, &bytes, import_password_option(password))?;
    let incoming = parsed.connections;
    let incoming_groups = parsed.groups;
    let skipped = parsed.skipped;
    let source_format = format_label(parsed.format).to_string();

    let result = apply_connection_import_impl(
        state,
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
        path = %source.display(),
        "import_connections OK"
    );
    Ok(result)
}

/// Open the native file picker for connection import (file only; no parse).
#[tauri::command]
pub async fn pick_connections_import_file(app: AppHandle) -> Result<Option<String>, CommandError> {
    let picked = super::dialog::open_file(&app, connections_open_filters()).await?;
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

/// Import connections from a path already chosen in the UI (TablePlus-style:
/// pick file first, then decrypt/import with the password entered afterward).
#[tauri::command]
pub async fn import_connections_at_path(
    state: State<'_, AppState>,
    password: String,
    path: String,
) -> Result<ImportConnectionsResult, CommandError> {
    let password = password.trim().to_string();
    let source = PathBuf::from(path);
    import_connections_from_path(&state, &password, source).await
}

/// Parse + decrypt + merge-import a connections file picked through the native
/// open dialog. Returns stats if imported, `None` if cancelled.
/// Password may be empty for DataGrip / Navicat / DBeaver / DBX plain JSON.
///
/// Decision 3 form: `override_path` (webdriver builds only) replaces the
/// dialog-picked file for E2E; production callers omit it.
#[tauri::command]
pub async fn import_connections_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    password: String,
    override_path: Option<String>,
) -> Result<Option<ImportConnectionsResult>, CommandError> {
    let password = password.trim().to_string();
    let source = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => Some(path),
        None => super::dialog::open_file(&app, connections_open_filters()).await?,
    };
    let Some(source) = source else {
        return Ok(None); // user dismissed the dialog
    };
    Ok(Some(import_connections_from_path(&state, &password, source).await?))
}

// NOTE: remaining production commands (encryption key, app data archive export/import,
// restart, etc.) continue below in the full production body that was split from tests.
// The full content is restored from the verified production extract.
