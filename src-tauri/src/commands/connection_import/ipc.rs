//! Connection import / export IPC surface.
//!
//! The format parsers live in the sibling modules; this file owns the commands
//! and the orchestration around them (path detection, dialogs, preview, merge).
//! Grouped here so the whole "move connections in and out" feature reads as one
//! module instead of being spread across `config.rs`.

use super::super::error::{resolve_override_path, CmdExt, CommandError, OVERRIDE_DISABLED_MSG};
use super::super::AppState;
use super::{
    build_tableplus_export, detect_import_path, format_label, parse_from_app, parse_import_file,
    ImportApp, ParsedImport, PathContext,
};
use crate::db::ConnectionConfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use tauri::{AppHandle, State};

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

pub(super) fn import_file_filters(app: ImportApp) -> (&'static str, &'static [&'static str]) {
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
        super::super::dialog::pick_folder(&app).await?
    } else {
        let (label, exts) = import_file_filters(import_app);
        super::super::dialog::open_file(
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
        return Err(CommandError::Validation("Password is required".into()));
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
    build_tableplus_export(connections, password)
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
            super::super::dialog::save_file(
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

fn build_import_preview_json(parsed: &ParsedImport) -> serde_json::Value {
    serde_json::json!({
        "connections": parsed.connections,
        "groups": parsed.groups,
        "skipped": parsed.skipped,
        "sourceFormat": format_label(parsed.format),
    })
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
        None => super::super::dialog::open_file(&app, connections_open_filters()).await?,
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
    let picked = super::super::dialog::open_file(&app, connections_open_filters()).await?;
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
        None => super::super::dialog::open_file(&app, connections_open_filters()).await?,
    };
    let Some(source) = source else {
        return Ok(None); // user dismissed the dialog
    };

    Ok(Some(
        import_connections_from_path(&state, &password, source).await?,
    ))
}

#[cfg(test)]
#[path = "ipc_tests.rs"]
mod tests;
