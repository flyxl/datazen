//! Skills listing IPC. Discovery and path policy live in the shared module;
//! the host only reads the persisted settings and projects the listing DTO.

use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen::ai::api as ai_api;
use datazen::ai::skills::{SkillCatalog, SkillSummary};
use tauri::State;

#[tauri::command]
pub async fn ai_list_skills(state: State<'_, AppState>) -> Result<Vec<SkillSummary>, CommandError> {
    let settings = ai_api::get_settings_config(state.store.as_ref()).await;
    Ok(SkillCatalog::from_settings(&settings.skills).list())
}
