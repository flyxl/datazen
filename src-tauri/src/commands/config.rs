use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::store::AppSettings;
use std::path::PathBuf;
use tauri::State;

pub(crate) async fn get_groups_impl(state: &AppState) -> Result<Vec<String>, CommandError> {
    let scope = state.connection_scope()?;
    let service = crate::platform::ipc::require_connection_service(state)?;
    match service.groups(&scope).await {
        Ok(groups) => Ok(groups),
        Err(datazen::connection::ConnectionError::NotFound(entity)) => {
            Err(CommandError::NotFound(entity))
        }
        Err(other) => Err(CommandError::Internal(other.to_string())),
    }
}

pub(crate) async fn save_groups_impl(
    state: &AppState,
    groups: Vec<String>,
) -> Result<(), CommandError> {
    tracing::info!(count = groups.len(), "save_groups");
    let scope = state.connection_scope()?;
    let service = crate::platform::ipc::require_connection_service(state)?;
    match service.save_groups(&scope, groups).await {
        Ok(()) => Ok(()),
        Err(datazen::connection::ConnectionError::NotFound(entity)) => {
            Err(CommandError::NotFound(entity))
        }
        Err(other) => Err(CommandError::Internal(other.to_string())),
    }
}

pub(crate) async fn get_host_settings_impl(state: &AppState) -> Result<AppSettings, CommandError> {
    let settings_service = crate::platform::ipc::require_settings_service(state)?;
    let value = settings_service
        .read_app_settings()
        .await
        .map_err(|_| CommandError::Internal("Unable to read settings".into()))?;
    serde_json::from_value(value)
        .map_err(|_| CommandError::Internal("Unable to read settings".into()))
}

pub(crate) async fn save_host_settings_impl(
    state: &AppState,
    mut settings: AppSettings,
) -> Result<(), CommandError> {
    let shared = datazen::settings::SettingsService::normalize(settings.shared_settings())
        .map_err(|error| CommandError::Validation(error.to_string()))?;
    settings
        .apply_shared_settings(shared)
        .map_err(CommandError::Validation)?;
    tracing::debug!(theme_mode = %settings.theme.mode, "save_host_settings");
    let settings_service = crate::platform::ipc::require_settings_service(state)?;
    let value = serde_json::to_value(&settings)
        .map_err(|_| CommandError::Internal("Unable to persist settings".into()))?;
    settings_service
        .save_app_settings(value)
        .await
        .map_err(|_| CommandError::Internal("Unable to persist settings".into()))?;
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
        .cmd_err("save_host_settings")?;
    if let Some(app) = state.monitor_engine.app_handle() {
        crate::tray::sync_tray_async(&app).await;
    }
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveSettingsRequest {
    pub settings: datazen::settings::SharedSettings,
}

/// Kernel-track settings read (form-split F4: renamed from `get_shared_settings`
/// — the neutral name now belongs to the kernel surface, 1:1 with the frontend
/// `getSettings` facade method; the legacy flat-blob commands live on as the
/// explicitly-named `get_host_settings` / `save_host_settings` host pair).
#[tauri::command]
pub async fn get_settings(
    state: State<'_, AppState>,
) -> Result<datazen::settings::SharedSettings, CommandError> {
    get_settings_impl(state.inner()).await
}

pub(crate) async fn get_settings_impl(
    state: &AppState,
) -> Result<datazen::settings::SharedSettings, CommandError> {
    // 设置用例自 D1 收尾后由 `datazen::settings` 拥有，宿主 adapter
    // 持有同一个服务实例，命令只调用例。这里**不得**再直读宿主 store——
    // 宿主直读会留下第二条事实源。
    let settings_service = crate::platform::ipc::require_settings_service(state)?;
    settings_service
        .read()
        .await
        .map_err(|_| CommandError::Internal("Unable to read settings".into()))
}

/// Kernel-track settings save (renamed from `save_shared_settings`).
#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    request: SaveSettingsRequest,
) -> Result<datazen::settings::SharedSettings, CommandError> {
    save_settings_impl(state.inner(), request.settings).await
}

pub(crate) async fn save_settings_impl(
    state: &AppState,
    settings: datazen::settings::SharedSettings,
) -> Result<datazen::settings::SharedSettings, CommandError> {
    // 同上：写入也走模块用例，宿主的 store 只作为端口实现被注入。
    let settings_service = crate::platform::ipc::require_settings_service(state)?;
    let saved = settings_service
        .save(settings)
        .await
        .map_err(|error| match error {
            datazen::settings::SettingsError::Validation(message) => {
                CommandError::Validation(message)
            }
            _ => CommandError::Internal("Unable to persist settings".into()),
        })?;
    // Form-split F4: monitor is a shared settings field now, so the monitor
    // engine reload and the tray sync belong on the kernel save path (they used
    // to ride the legacy flat-blob save only).
    let full_settings = get_host_settings_impl(state).await?;
    crate::redis_flush_gate::sync_from_settings(&full_settings);
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
    Ok(saved)
}

pub(crate) async fn get_log_path_impl(state: &AppState) -> Result<String, CommandError> {
    let settings = get_host_settings_impl(state).await?;
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
/// Host-track settings read (form-split F4: renamed from the legacy flat-blob
/// `get_settings` — the full desktop field set feeding the `DesktopSettings`
/// projection; the neutral kernel names above serve the shared contract).
#[tauri::command]
pub async fn get_host_settings(state: State<'_, AppState>) -> Result<AppSettings, CommandError> {
    get_host_settings_impl(&state).await
}

/// Host-track settings save (renamed from the legacy flat-blob `save_settings`).
#[tauri::command]
pub async fn save_host_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<(), CommandError> {
    save_host_settings_impl(&state, settings).await
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

/// Open a well-known host directory (`kind`: `log` / `workflows` / `context`).
///
/// Form-split F4c merge of the three named openers (`open_log_dir` /
/// `open_workflows_dir` / `open_context_dir`): the path stays server-resolved —
/// the webview only names the well-known directory, never a filesystem path.
#[tauri::command]
pub async fn open_known_dir(state: State<'_, AppState>, kind: String) -> Result<(), CommandError> {
    let dir = match kind.trim() {
        "log" => {
            let settings = get_host_settings_impl(state.inner()).await?;
            crate::resolve_log_dir(state.store.data_dir(), &settings.log_path)
        }
        "workflows" => state.workflow_registry.workflows_dir().clone(),
        "context" => {
            let settings = get_host_settings_impl(state.inner()).await?;
            crate::resolve_context_dir(state.store.data_dir(), &settings.context_dir)
        }
        other => {
            return Err(CommandError::Validation(format!(
                "open_known_dir: unknown kind '{other}' (expected log, workflows, or context)"
            )))
        }
    };
    std::fs::create_dir_all(&dir).map_err(CommandError::from)?;
    open::that(&dir).map_err(|e| CommandError::Internal(format!("open_known_dir: {e}")))
}

/// Open a path only if it lies under the app data dir or configured context dir.
/// Prefer `open_known_dir` when the target is one of the well-known directories.
pub(crate) async fn open_path_impl(state: &AppState, path: String) -> Result<(), CommandError> {
    if path.starts_with("http://") || path.starts_with("https://") {
        return open::that(&path).map_err(|e| CommandError::Internal(format!("open_path: {e}")));
    }

    require_webdriver_path_ipc("open_path disabled; use open_known_dir")?;
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

    let settings = get_host_settings_impl(state).await?;
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

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
