//! AI-related Tauri IPC commands.

use crate::ai::*;
use crate::commands::{log_err, AppState};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListItem {
    pub provider_type: AiProviderType,
    pub display_name: String,
    pub models: Vec<ModelInfo>,
    pub default_model: String,
    pub supports_streaming: bool,
    pub supports_tools: bool,
}

#[tauri::command]
pub async fn ai_get_providers(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderListItem>, String> {
    let providers = state.ai_registry.all_providers().await;
    let result = providers
        .into_iter()
        .map(|p| ProviderListItem {
            provider_type: p.provider_type(),
            display_name: p.display_name().to_string(),
            models: p.available_models(),
            default_model: p.default_model().to_string(),
            supports_streaming: p.supports_streaming(),
            supports_tools: p.supports_tools(),
        })
        .collect();
    Ok(result)
}

#[tauri::command]
pub async fn ai_get_models(
    state: State<'_, AppState>,
    provider_type: AiProviderType,
) -> Result<Vec<ModelInfo>, String> {
    let provider = state
        .ai_registry
        .get(&provider_type)
        .await
        .ok_or_else(|| format!("Provider {:?} not found", provider_type))?;
    Ok(provider.available_models())
}

#[tauri::command]
pub async fn ai_validate_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), String> {
    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| format!("Provider {:?} not found", config.provider_type))?;
    provider
        .validate_config(&config)
        .await
        .map_err(|e| log_err("ai_validate_config", &e))
}

#[tauri::command]
pub async fn ai_save_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), String> {
    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| format!("Provider {:?} not found", config.provider_type))?;

    provider
        .initialize(&config)
        .await
        .map_err(|e| log_err("ai_save_config", &e))?;

    state
        .store
        .save_ai_config(&config)
        .await
        .map_err(|e| log_err("ai_save_config", &e))
}

#[tauri::command]
pub async fn ai_get_config(
    state: State<'_, AppState>,
) -> Result<Option<AiProviderConfig>, String> {
    Ok(state.store.get_ai_config().await)
}

#[tauri::command]
pub async fn ai_delete_config(
    state: State<'_, AppState>,
) -> Result<(), String> {
    for provider in state.ai_registry.all_providers().await {
        provider.reset().await;
    }
    state
        .store
        .delete_ai_config()
        .await
        .map_err(|e| log_err("ai_delete_config", &e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_list_item_serialization() {
        let item = ProviderListItem {
            provider_type: AiProviderType::OpenAi,
            display_name: "OpenAI".into(),
            models: vec![ModelInfo {
                id: "gpt-4o".into(),
                display_name: "GPT-4o".into(),
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: true,
            }],
            default_model: "gpt-4o".into(),
            supports_streaming: true,
            supports_tools: true,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"providerType\":\"open_ai\""));
        assert!(json.contains("\"displayName\":\"OpenAI\""));
        assert!(json.contains("\"defaultModel\":\"gpt-4o\""));
        assert!(json.contains("\"supportsStreaming\":true"));
    }
}
