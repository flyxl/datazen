//! Prompt template management IPC commands.
//!
//! 命令层只做接线：懒启动与模板装载在 `datazen::ai::api::ensure_ai_ready`，
//! 列表 / 覆盖 / 解除覆盖在 `PromptResolver`（modules）；宿主只把驱动
//! 注册表按名字查到的句柄递给用例（接线性查询，不是编排）。

use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen::ai::prompt_resolver::{PromptInfo, PromptOverrideEntry};
use datazen_driver_api::PromptScenario;
use tauri::State;

// ─── Prompt management IPC commands ───

#[tauri::command]
pub async fn prompt_list(
    state: State<'_, AppState>,
    driver_type: Option<String>,
) -> Result<Vec<PromptInfo>, CommandError> {
    state.ensure_ai_ready().await;
    let driver: Option<std::sync::Arc<dyn datazen_driver_api::DatabaseDriver>> =
        if let Some(ref dt) = driver_type {
            state.driver_registry.get_sql_driver_by_name(dt).await
        } else {
            None
        };
    let prompts = state.prompt_resolver.list_prompts(driver.as_deref()).await;
    Ok(prompts)
}

#[tauri::command]
pub async fn prompt_set_override(
    state: State<'_, AppState>,
    entry: PromptOverrideEntry,
) -> Result<(), CommandError> {
    state
        .prompt_resolver
        .set_override(entry)
        .await
        .map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn prompt_remove_override(
    state: State<'_, AppState>,
    driver_type: String,
    scenario: PromptScenario,
) -> Result<(), CommandError> {
    state
        .prompt_resolver
        .remove_override(&driver_type, scenario)
        .await
        .map_err(CommandError::Internal)
}
