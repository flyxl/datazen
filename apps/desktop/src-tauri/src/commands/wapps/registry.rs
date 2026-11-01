//! Wapp registry and lifecycle: list, manifest, inspect, install, remove,
//! enable.
//!
//! B2 wapps 域刀：`inspect_wapp_package_impl` 转发体已删（接线内联进
//! `inspect_wapp_package_with_dialog`）；其余五个 `*_impl` 因跨域调用方
//! （`src/wapps/integration_tests.rs`，协议桥集成测试）保留为域内共享入口，
//! 各自的调用方清单见函数文档。`WappSummary` 投影本体已下沉
//! `datazen::workspace_apps::summary`。

use super::*;
use serde::Serialize;

/// 跨域共享入口（B2 域刀保留）：生产调用方 `list_wapps` 命令；跨域测试
/// 调用方 `src/wapps/integration_tests.rs`（经 `crate::commands::
/// list_wapps_impl` 直调）+ 本域 `tests.rs`。安装/卸载/启停命令在事件
/// 发射（`wapps:changed`）后共享同一实现。
pub(crate) fn list_wapps_impl(state: &AppState) -> Vec<WappSummary> {
    state.wapps.list().iter().map(WappSummary::from).collect()
}

/// 跨域共享入口（B2 域刀保留）：生产调用方 `get_wapp_manifest` 命令；
/// 跨域测试调用方 `src/wapps/integration_tests.rs` + 本域 `tests.rs`。
pub(crate) fn get_wapp_manifest_impl(
    state: &AppState,
    id: &str,
) -> Result<WappManifest, CommandError> {
    ensure_wapp_exists(state, id).map(|loaded| loaded.manifest)
}

/// 跨域共享入口（B2 域刀保留）：生产调用方 `install_wapp` 命令（pick
/// token/override path 解析是宿主形态，留在命令里）；跨域测试调用方
/// `src/wapps/integration_tests.rs` + 本域 `tests.rs` 的 `install_zip` 夹具。
pub(crate) async fn install_wapp_from_path_impl(
    state: &AppState,
    path: String,
) -> Result<WappSummary, CommandError> {
    let source = PathBuf::from(&path);
    if !source.exists() {
        return Err(CommandError::NotFound(format!(
            "wapp package not found: {}",
            source.display()
        )));
    }

    let wapps_dir = state.wapps.wapps_dir().to_path_buf();
    let manifest = tokio::task::spawn_blocking(move || {
        let is_zip = source.is_file()
            && source
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
        if is_zip {
            install_from_zip(&source, &wapps_dir)
        } else {
            install_from_dir(&source, &wapps_dir)
        }
    })
    .await
    .map_err(|e| CommandError::Internal(format!("install_wapp_from_path task: {e}")))?
    .map_err(CommandError::Validation)?;

    state
        .wapps
        .register(manifest.clone(), true)
        .map_err(CommandError::Validation)?;
    tracing::info!(id = %manifest.id, version = %manifest.version, "install_wapp_from_path OK");

    Ok(WappSummary::from(&LoadedWapp {
        manifest,
        enabled: true,
    }))
}

/// 跨域共享入口（B2 域刀保留）：生产调用方 `remove_wapp` 命令；跨域测试
/// 调用方 `src/wapps/integration_tests.rs` + 本域 `tests.rs`。
pub(crate) async fn remove_wapp_impl(state: &AppState, id: String) -> Result<(), CommandError> {
    ensure_wapp_exists(state, &id)?;

    let manager = state.wapps.clone();
    let removed_id = id.clone();
    tokio::task::spawn_blocking(move || manager.remove(&removed_id))
        .await
        .map_err(|e| CommandError::Internal(format!("remove_wapp task: {e}")))?
        .map_err(CommandError::Validation)?;

    super::sessions::end_wapp_instances(&id);
    tracing::info!(%id, "remove_wapp OK");
    Ok(())
}

/// 跨域共享入口（B2 域刀保留）：生产调用方 `set_wapp_enabled` 命令；跨域
/// 测试调用方 `src/wapps/integration_tests.rs` + 本域 `tests.rs`。禁用时
/// 撤销该 Wapp 的全部实例（会话语义见 sessions.rs）。
pub(crate) async fn set_wapp_enabled_impl(
    state: &AppState,
    id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    ensure_wapp_exists(state, &id)?;

    let manager = state.wapps.clone();
    let toggled_id = id.clone();
    tokio::task::spawn_blocking(move || manager.set_enabled(&toggled_id, enabled))
        .await
        .map_err(|e| CommandError::Internal(format!("set_wapp_enabled task: {e}")))?
        .map_err(CommandError::Validation)?;

    if !enabled {
        super::sessions::end_wapp_instances(&id);
    }
    tracing::info!(%id, %enabled, "set_wapp_enabled OK");
    Ok(())
}

#[tauri::command]
pub async fn list_wapps(state: State<'_, AppState>) -> Result<Vec<WappSummary>, CommandError> {
    Ok(list_wapps_impl(&state))
}

#[tauri::command]
pub async fn get_wapp_manifest(
    state: State<'_, AppState>,
    id: String,
) -> Result<WappManifest, CommandError> {
    get_wapp_manifest_impl(&state, &id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappPackagePreview {
    pub pick_token: String,
    pub package_label: String,
    pub manifest: WappManifest,
}

/// 接线（B2 域刀自 `inspect_wapp_package_impl` 内联）：内核的完整包校验
/// （`datazen::workspace_apps::install::inspect_wapp_package`——不落盘、
/// 物化进临时目录后跑同一套规则 1–7）在阻塞线程上执行；错误映射保持
/// NotFound（路径缺失，上方预检）/ Validation（校验失败）。
#[tauri::command]
pub async fn inspect_wapp_package_with_dialog(
    app: AppHandle,
    package_kind: String,
    override_path: Option<String>,
) -> Result<Option<WappPackagePreview>, CommandError> {
    let kind = WappPackageKind::parse(&package_kind).map_err(CommandError::Validation)?;
    let Some(source) = resolve_wapp_package_path(&app, kind, override_path).await? else {
        return Ok(None);
    };
    if !source.exists() {
        return Err(CommandError::NotFound(format!(
            "wapp package not found: {}",
            source.display()
        )));
    }

    let manifest = tokio::task::spawn_blocking(move || {
        datazen::workspace_apps::install::inspect_wapp_package(&source)
    })
    .await
    .map_err(|e| CommandError::Internal(format!("inspect_wapp_package task: {e}")))?
    .map_err(CommandError::Validation)?;
    let pick_token = insert_pick_session(source.clone()).await?;
    Ok(Some(WappPackagePreview {
        pick_token,
        package_label: package_label(&source),
        manifest,
    }))
}

#[tauri::command]
pub async fn install_wapp(
    app: AppHandle,
    state: State<'_, AppState>,
    pick_token: Option<String>,
    override_path: Option<String>,
) -> Result<WappSummary, CommandError> {
    let path = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => path.to_string_lossy().into_owned(),
        None => {
            let Some(token) = pick_token else {
                return Err(CommandError::Validation("No wapp package selected".into()));
            };
            take_pick_session(&token)
                .await?
                .to_string_lossy()
                .into_owned()
        }
    };

    let summary = install_wapp_from_path_impl(&state, path).await?;
    let _ = app.emit(WAPPS_CHANGED_EVENT, ());
    Ok(summary)
}

#[tauri::command]
pub async fn remove_wapp(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    remove_wapp_impl(&state, id).await?;
    let _ = app.emit(WAPPS_CHANGED_EVENT, ());
    Ok(())
}

#[tauri::command]
pub async fn wapp_audit_log(
    wapp_id: String,
    event: String,
    detail: String,
) -> Result<(), CommandError> {
    let id = wapp_id;
    if id.is_empty() || id.chars().count() > 64 {
        return Err(CommandError::Validation("invalid wapp_id".into()));
    }
    let event = event.chars().take(64).collect::<String>();
    let detail = detail.chars().take(200).collect::<String>();
    tracing::info!(
        target: "wapp_audit",
        wapp_id = %id,
        event = %event,
        detail = %detail,
        "ui-wapp audit"
    );
    Ok(())
}

#[tauri::command]
pub async fn set_wapp_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    set_wapp_enabled_impl(&state, id, enabled).await?;
    let _ = app.emit(WAPPS_CHANGED_EVENT, ());
    Ok(())
}
