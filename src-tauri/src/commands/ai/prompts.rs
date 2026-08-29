//! Prompt template management IPC commands.

use crate::ai::prompt_resolver::{PromptInfo, PromptOverrideEntry};
use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen_driver_api::PromptScenario;
use tauri::State;

// ─── Prompt management IPC commands ───

pub(crate) async fn prompt_list_impl(
    state: &AppState,
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
pub async fn prompt_list(
    state: State<'_, AppState>,
    driver_type: Option<String>,
) -> Result<Vec<PromptInfo>, CommandError> {
    prompt_list_impl(&state, driver_type).await
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
