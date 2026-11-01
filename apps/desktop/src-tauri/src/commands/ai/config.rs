//! AI provider configuration IPC commands.
//!
//! host-boundary §3（F4a `connection/api` 先例）：命令层只做请求接线与
//! 转发——把 registry、[`AiConfigStore`] 端口与请求 DTO 交给
//! `datazen::ai::api`，编排（供应商懒注册、持久化配置恢复、保存时的
//! initialize 顺序、删除时的 reset 顺序）在 modules 定稿。桌面在这里保留
//! 的只有传输胶水：`ai:config-changed` 事件发射与命令名日志；错误经
//! `AiUseCaseError → CommandError` 的同桶映射，IPC 文案不变。
//!
//! 接线逻辑直接住在命令处理函数里（WR-A2：无 `_impl` 转发层）；带事件
//! 发射的四个命令对 `R: Runtime` 泛型，测试经 `tauri::test::mock_app`
//! 注入 MockRuntime 宿主直调。

use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use datazen::ai::api as ai_api;
use datazen::ai::{AiProviderConfig, AiSettingsConfig, ModelInfo};
use tauri::{AppHandle, Emitter, State};

// 展示 DTO（含内建端点 / 协议默认值映射）归模块所有；宿主再导出保持
// `commands::ProviderListItem` 旧路径可用（宿主测试与前端类型面不变）。
pub use datazen::ai::api::ProviderListItem;

#[tauri::command]
pub async fn ai_get_providers(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderListItem>, CommandError> {
    state.ensure_ai_ready().await;
    Ok(ai_api::list_providers(&state.ai_registry).await)
}

#[tauri::command]
pub async fn ai_fetch_remote_models(
    protocol: String,
    endpoint: String,
    api_key: String,
) -> Result<Vec<ModelInfo>, CommandError> {
    // 协议字符串是 IPC 词汇，映射（含未知词表值的拒绝文案）在用例层定稿。
    ai_api::fetch_remote_models(&protocol, &endpoint, &api_key)
        .await
        .cmd_err("ai_fetch_remote_models")
}

#[tauri::command]
pub async fn ai_validate_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    state.ensure_ai_ready().await;
    ai_api::validate_provider_config(&state.ai_registry, &config)
        .await
        .cmd_err("ai_validate_config")
}

#[tauri::command]
pub async fn ai_save_config<R: tauri::Runtime>(
    handle: AppHandle<R>,
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    state.ensure_ai_ready().await;
    ai_api::save_provider_config(&state.ai_registry, state.store.as_ref(), config)
        .await
        .cmd_err("ai_save_config")?;
    let _ = handle.emit("ai:config-changed", true);
    Ok(())
}

#[tauri::command]
pub async fn ai_get_config(
    state: State<'_, AppState>,
) -> Result<Option<AiProviderConfig>, CommandError> {
    Ok(ai_api::get_provider_config(state.store.as_ref()).await)
}

#[tauri::command]
pub async fn ai_delete_config<R: tauri::Runtime>(
    handle: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    ai_api::delete_provider_config(&state.ai_registry, state.store.as_ref())
        .await
        .cmd_err("ai_delete_config")?;
    let _ = handle.emit("ai:config-changed", false);
    Ok(())
}

#[tauri::command]
pub async fn ai_get_settings_config(
    state: State<'_, AppState>,
) -> Result<AiSettingsConfig, CommandError> {
    Ok(ai_api::get_settings_config(state.store.as_ref()).await)
}

#[tauri::command]
pub async fn ai_save_settings_config<R: tauri::Runtime>(
    handle: AppHandle<R>,
    state: State<'_, AppState>,
    config: AiSettingsConfig,
) -> Result<(), CommandError> {
    state.ensure_ai_ready().await;
    ai_api::save_settings_config(&state.ai_registry, state.store.as_ref(), config)
        .await
        .cmd_err("ai_save_settings_config")?;
    let _ = handle.emit("ai:config-changed", true);
    Ok(())
}

#[tauri::command]
pub async fn ai_set_active_profile<R: tauri::Runtime>(
    handle: AppHandle<R>,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), CommandError> {
    state.ensure_ai_ready().await;
    ai_api::set_active_profile(&state.ai_registry, state.store.as_ref(), &profile_id)
        .await
        .cmd_err("ai_set_active_profile")?;
    let _ = handle.emit("ai:config-changed", true);
    Ok(())
}
