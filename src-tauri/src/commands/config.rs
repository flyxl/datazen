use super::connection_import::{self, format_label, parse_import_file};
use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::app_data_archive;
use crate::db::ConnectionConfig;
use crate::i18n_locale;
use crate::store::AppSettings;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn get_groups(state: State<'_, AppState>) -> Result<Vec<String>, CommandError> {
    Ok(state.store.get_groups().await)
}

#[tauri::command]
pub async fn save_groups(state: State<'_, AppState>, groups: Vec<String>) -> Result<(), CommandError> {
    tracing::info!(count = groups.len(), "save_groups");
    state
        .store
        .save_groups(groups)
        .await
        .cmd_err("save_groups")
}
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, CommandError> {
    Ok(state.store.get_settings().await)
}

#[tauri::command]
pub fn get_system_ui_language() -> String {
    i18n_locale::default_ui_language()
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), CommandError> {
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
        .map_err(|e| CommandError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )))
        .cmd_err("save_settings")?;
    if let Some(app) = state.monitor_engine.app_handle() {
        crate::tray::sync_tray_async(&app).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_log_path(state: State<'_, AppState>) -> Result<String, CommandError> {
    let settings = state.store.get_settings().await;
    let data_dir = state.store.data_dir();
    let log_dir = if settings.log_path.is_empty() {
        data_dir.join("logs")
    } else {
        PathBuf::from(&settings.log_path)
    };
    Ok(log_dir.to_string_lossy().to_string())
}

fn path_is_under(child: &std::path::Path, root: &std::path::Path) -> bool {
    child.starts_with(root)
}

/// Legacy path-based IPC is only available in webdriver/E2E builds.
fn require_webdriver_path_ipc(disabled_msg: &'static str) -> Result<(), CommandError> {
    if !cfg!(feature = "webdriver") {
        return Err(CommandError::Validation(disabled_msg.into()));
    }
    Ok(())
}

/// Open the application log directory (path resolved server-side).
#[tauri::command]
pub async fn open_log_dir(state: State<'_, AppState>) -> Result<(), CommandError> {
    let settings = state.store.get_settings().await;
    let data_dir = state.store.data_dir();
    let log_dir = if settings.log_path.is_empty() {
        data_dir.join("logs")
    } else {
        PathBuf::from(&settings.log_path)
    };
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
    let context_dir = if settings.context_dir.is_empty() {
        data_dir.join("contexts")
    } else {
        PathBuf::from(&settings.context_dir)
    };
    std::fs::create_dir_all(&context_dir).map_err(CommandError::from)?;
    open::that(&context_dir).map_err(|e| CommandError::Internal(format!("open_context_dir: {e}")))
}

/// Open a path only if it lies under the app data dir or configured context dir.
/// Prefer open_log_dir / open_workflows_dir / open_context_dir when possible.
#[tauri::command]
pub async fn open_path(state: State<'_, AppState>, path: String) -> Result<(), CommandError> {
    require_webdriver_path_ipc(
        "open_path disabled; use open_log_dir / open_workflows_dir / open_context_dir",
    )?;
    let requested = PathBuf::from(&path);
    if requested.to_string_lossy().contains("..") {
        return Err(CommandError::Validation("Path traversal not allowed".into()));
    }

    let data_dir = state.store.data_dir().clone();
    if !requested.exists() && path_is_under(&requested, &data_dir) {
        std::fs::create_dir_all(&requested).map_err(CommandError::from)?;
    }

    let canonical = requested
        .canonicalize()
        .map_err(|e| CommandError::Validation(format!("Cannot resolve path: {e}")))?;
    let data_canon = data_dir
        .canonicalize()
        .unwrap_or(data_dir.clone());

    let settings = state.store.get_settings().await;
    let context_root = if settings.context_dir.is_empty() {
        data_dir.join("contexts")
    } else {
        PathBuf::from(&settings.context_dir)
    };
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
    groups: &[String],
    password: &str,
) -> Result<String, CommandError> {
    validate_share_password(password)?;
    connection_import::build_encrypted_export(connections, groups, password)
}

#[tauri::command]
pub async fn export_connections(
    state: State<'_, AppState>,
    path: String,
    password: String,
) -> Result<u32, CommandError> {
    require_webdriver_path_ipc("Direct path connection export disabled")?;
    tracing::info!(%path, "export_connections");
    let connections = state.store.get_connections().await;
    let groups = state.store.get_groups().await;
    let count = connections.len() as u32;

    let json = build_encrypted_connections_export(&connections, &groups, &password)?;

    tokio::fs::write(PathBuf::from(&path), json.as_bytes())
        .await
        .cmd_err("export_connections")?;

    tracing::info!(%path, count, "export_connections OK");
    Ok(count)
}

/// Native save dialog + encrypted JSON export. Returns connection count if saved, `None` if cancelled.
#[tauri::command]
pub async fn export_connections_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    password: String,
    default_file_name: String,
) -> Result<Option<u32>, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    validate_share_password(&password)?;

    let picked = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name(&default_file_name)
        .blocking_save_file();
    let Some(fp) = picked else {
        return Ok(None);
    };
    let dest = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;

    let connections = state.store.get_connections().await;
    let groups = state.store.get_groups().await;
    let count = connections.len() as u32;
    let json = build_encrypted_connections_export(&connections, &groups, &password)?;

    tokio::fs::write(dest, json.as_bytes())
        .await
        .cmd_err("export_connections_with_dialog")?;

    tracing::info!(count, "export_connections_with_dialog OK");
    Ok(Some(count))
}

#[tauri::command]
pub async fn import_connections_preview(
    path: String,
    password: String,
) -> Result<serde_json::Value, CommandError> {
    require_webdriver_path_ipc("Direct path connection import disabled")?;
    tracing::info!(%path, "import_connections_preview");
    let source = PathBuf::from(&path);
    let bytes = tokio::fs::read(&source)
        .await
        .cmd_err("import_connections_preview")?;

    let password_opt = if password.trim().is_empty() {
        None
    } else {
        Some(password.as_str())
    };
    let parsed = parse_import_file(&source, &bytes, password_opt)?;

    Ok(serde_json::json!({
        "connections": parsed.connections,
        "groups": parsed.groups,
        "skipped": parsed.skipped,
        "sourceFormat": format_label(parsed.format),
    }))
}

/// Native open dialog + decrypt/merge import. Returns stats if imported, `None` if cancelled.
/// Password may be empty for DataGrip / Navicat / DBeaver / DBX plain JSON.
#[tauri::command]
pub async fn import_connections_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    password: String,
) -> Result<Option<ImportConnectionsResult>, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .add_filter(
            "Connections",
            &["json", "xml", "ncx", "tableplusconnection"],
        )
        .add_filter("DataZen / DBX JSON", &["json"])
        .add_filter("DataGrip XML", &["xml"])
        .add_filter("Navicat NCX", &["ncx", "xml"])
        .add_filter("DBeaver JSON", &["json"])
        .add_filter("TablePlus", &["tableplusconnection"])
        .blocking_pick_file();
    let Some(fp) = picked else {
        return Ok(None);
    };
    let source = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;

    let bytes = tokio::fs::read(&source)
        .await
        .cmd_err("import_connections_with_dialog")?;

    let password_opt = if password.trim().is_empty() {
        None
    } else {
        Some(password.as_str())
    };
    let parsed = parse_import_file(&source, &bytes, password_opt)?;
    let incoming = parsed.connections;
    let incoming_groups = parsed.groups;
    let skipped = parsed.skipped;
    let source_format = format_label(parsed.format).to_string();

    let existing = state.store.get_connections().await;
    let existing_ids: HashSet<String> = existing.iter().map(|c| c.id.clone()).collect();
    let (imported, overwritten) = merge_connection_import_stats(&existing_ids, &incoming);

    for conn in incoming {
        state.store.save_connection(conn).await?;
    }

    let existing_groups = state.store.get_groups().await;
    let (merged_groups, groups_added) = merge_group_lists(&existing_groups, &incoming_groups);
    state.store.save_groups(merged_groups).await?;

    tracing::info!(
        imported,
        overwritten,
        groups_added,
        skipped = skipped.len(),
        %source_format,
        "import_connections_with_dialog OK"
    );
    Ok(Some(ImportConnectionsResult {
        imported,
        overwritten,
        groups_added,
        skipped,
        source_format,
    }))
}

#[tauri::command]
pub async fn export_app_data(state: State<'_, AppState>, path: String) -> Result<(), CommandError> {
    require_webdriver_path_ipc("Direct path export disabled; use export_app_data_with_dialog")?;
    tracing::info!(%path, "export_app_data");
    let data_dir = state.store.data_dir().clone();
    let settings = state.store.get_settings().await;
    let options = app_data_archive::ExportOptions {
        include_dashboard_runs: settings.monitor.export_include_dashboard_runs,
    };
    let dest = PathBuf::from(path);
    tokio::task::spawn_blocking(move || {
        app_data_archive::export_app_data_with_options(&data_dir, &dest, options)
    })
        .await
        .map_err(|e| CommandError::Internal(format!("export_app_data task: {e}")))?
        .cmd_err("export_app_data")?;
    tracing::info!("export_app_data OK");
    Ok(())
}

/// Native save dialog + ZIP export. Path never crosses the webview.
/// Returns `true` if exported, `false` if cancelled.
#[tauri::command]
pub async fn export_app_data_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    default_file_name: String,
) -> Result<bool, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .add_filter("ZIP", &["zip"])
        .set_file_name(&default_file_name)
        .blocking_save_file();
    let Some(fp) = picked else {
        return Ok(false);
    };
    let dest = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;
    let data_dir = state.store.data_dir().clone();
    let settings = state.store.get_settings().await;
    let options = app_data_archive::ExportOptions {
        include_dashboard_runs: settings.monitor.export_include_dashboard_runs,
    };
    tokio::task::spawn_blocking(move || {
        app_data_archive::export_app_data_with_options(&data_dir, &dest, options)
    })
        .await
        .map_err(|e| CommandError::Internal(format!("export_app_data_with_dialog task: {e}")))?
        .cmd_err("export_app_data_with_dialog")?;
    Ok(true)
}

#[tauri::command]
pub async fn import_app_data(state: State<'_, AppState>, path: String) -> Result<(), CommandError> {
    require_webdriver_path_ipc("Direct path import disabled; use import_app_data_with_dialog")?;
    tracing::info!(%path, "import_app_data");
    let data_dir = state.store.data_dir().clone();
    let source = PathBuf::from(path);
    tokio::task::spawn_blocking(move || app_data_archive::import_app_data(&data_dir, &source))
        .await
        .map_err(|e| CommandError::Internal(format!("import_app_data task: {e}")))?
        .cmd_err("import_app_data")?;
    tracing::info!("import_app_data OK");
    Ok(())
}

/// Native open + confirm + ZIP import. Returns `true` if imported.
#[tauri::command]
pub async fn import_app_data_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    confirm_title: String,
    confirm_message: String,
) -> Result<bool, CommandError> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    let picked = app
        .dialog()
        .file()
        .add_filter("ZIP", &["zip"])
        .blocking_pick_file();
    let Some(fp) = picked else {
        return Ok(false);
    };
    let source = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;

    // OkCancelCustom: callback/blocking_show returns true when the first (OK) button is pressed.
    let confirmed = app
        .dialog()
        .message(&confirm_message)
        .title(&confirm_title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show();
    if !confirmed {
        return Ok(false);
    }

    let data_dir = state.store.data_dir().clone();
    tokio::task::spawn_blocking(move || app_data_archive::import_app_data(&data_dir, &source))
        .await
        .map_err(|e| CommandError::Internal(format!("import_app_data_with_dialog task: {e}")))?
        .cmd_err("import_app_data_with_dialog")?;
    Ok(true)
}

/// Bytes written when exporting the encryption key file (base64 text, trimmed).
pub fn encryption_key_export_bytes(key_b64: &str) -> Vec<u8> {
    key_b64.trim().as_bytes().to_vec()
}

/// Native save dialog for the app encryption key (`.key` material). Path never crosses the webview.
/// Returns `true` if saved, `false` if cancelled.
#[tauri::command]
pub async fn save_encryption_key_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    default_file_name: String,
) -> Result<bool, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let key_b64 = state.store.encryption_key_b64();
    let bytes = encryption_key_export_bytes(&key_b64);

    let picked = app
        .dialog()
        .file()
        .add_filter("Encryption Key", &["key"])
        .set_file_name(&default_file_name)
        .blocking_save_file();
    let Some(fp) = picked else {
        return Ok(false);
    };
    let dest = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn path_is_under_matches_prefix() {
        assert!(path_is_under(
            Path::new("/data/app/logs"),
            Path::new("/data/app")
        ));
        assert!(!path_is_under(
            Path::new("/tmp/evil"),
            Path::new("/data/app")
        ));
    }

    #[test]
    fn require_webdriver_path_ipc_gates_without_feature() {
        let result = require_webdriver_path_ipc("Direct path connection export disabled");
        if cfg!(feature = "webdriver") {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("disabled"));
        }
    }

    #[test]
    fn export_rejects_empty_password() {
        assert!(validate_share_password("").is_err());
        assert!(validate_share_password("   ").is_err());
        assert!(validate_share_password("secret").is_ok());
    }

    #[test]
    fn merge_connections_overwrites_by_id() {
        use crate::db::{ConnectionConfig, SslMode};

        fn conn(id: &str) -> ConnectionConfig {
            ConnectionConfig {
                id: id.into(),
                name: id.into(),
                database_type: "postgresql".into(),
                host: None,
                port: None,
                database: None,
                schema: None,
                username: None,
                password: None,
                ssl_mode: SslMode::default(),
                connection_timeout: 30,
                ssh_tunnel: None,
                color_tag: None,
                group: None,
                last_connected_at: None,
                server_version: None,
                options: None,
            }
        }

        let existing_ids: HashSet<String> = ["a", "b"].into_iter().map(String::from).collect();
        let incoming = vec![conn("b"), conn("c"), conn("d")];
        let (imported, overwritten) = merge_connection_import_stats(&existing_ids, &incoming);
        assert_eq!(imported, 2);
        assert_eq!(overwritten, 1);
    }

    #[test]
    fn merge_group_lists_unions_and_counts_new() {
        let (merged, added) = merge_group_lists(
            &["alpha".into(), "beta".into()],
            &["beta".into(), "gamma".into()],
        );
        assert_eq!(merged, vec!["alpha", "beta", "gamma"]);
        assert_eq!(added, 1);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        use super::connection_import::{decrypt_datazen_fields, derive_argon2_key, encrypt_field};
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

        let salt = [7u8; 16];
        let key = derive_argon2_key("unit-test-password", &salt).unwrap();
        let cipher = encrypt_field("secret-db-password", &key).unwrap();
        let mut data = serde_json::json!({
            "encrypted": true,
            "salt": BASE64.encode(salt),
            "connections": [{ "password": cipher }]
        });
        decrypt_datazen_fields(&mut data, "unit-test-password").unwrap();
        assert_eq!(data["connections"][0]["password"], "secret-db-password");
    }

    #[test]
    fn decrypt_rejects_wrong_password() {
        use super::connection_import::{decrypt_datazen_fields, derive_argon2_key, encrypt_field};
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

        let salt = [9u8; 16];
        let key = derive_argon2_key("correct", &salt).unwrap();
        let cipher = encrypt_field("payload", &key).unwrap();
        let mut data = serde_json::json!({
            "encrypted": true,
            "salt": BASE64.encode(salt),
            "connections": [{ "password": cipher }]
        });
        assert!(decrypt_datazen_fields(&mut data, "wrong").is_err());
    }

    #[test]
    fn encryption_key_export_bytes_roundtrip_write() {
        let key_b64 = "  dGVzdGtleQ==  \n";
        let bytes = encryption_key_export_bytes(key_b64);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("datazen.key");
        std::fs::write(&path, &bytes).unwrap();
        let read_back = std::fs::read_to_string(&path).unwrap();
        assert_eq!(read_back, "dGVzdGtleQ==");
    }
}
