//! AI provider configuration IPC commands.

use super::util::resolve_ai;
use crate::ai::*;
use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use datazen_ai_api::AiProviderConfig;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListItem {
    pub provider_type: AiProviderType,
    pub display_name: String,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub default_endpoint: String,
    pub default_protocol: String,
}

pub(crate) fn provider_defaults(pt: AiProviderType) -> (&'static str, &'static str) {
    match pt {
        AiProviderType::OpenAi => ("https://api.openai.com/v1", "open_ai_compatible"),
        AiProviderType::Anthropic => ("https://api.anthropic.com", "anthropic_compatible"),
        AiProviderType::DeepSeek => ("https://api.deepseek.com", "open_ai_responses"),
        AiProviderType::Ollama => ("http://127.0.0.1:11434/v1", "open_ai_compatible"),
        AiProviderType::Custom => ("", "open_ai_compatible"),
    }
}

pub(crate) async fn ai_get_providers_impl(
    state: &AppState,
) -> Result<Vec<ProviderListItem>, CommandError> {
    state.ensure_ai_ready().await;
    let providers = state.ai_registry.all_providers().await;
    let result = providers
        .into_iter()
        .map(|p| {
            let (ep, proto) = provider_defaults(p.provider_type());
            ProviderListItem {
                provider_type: p.provider_type(),
                display_name: p.display_name().to_string(),
                supports_streaming: p.supports_streaming(),
                supports_tools: p.supports_tools(),
                default_endpoint: ep.into(),
                default_protocol: proto.into(),
            }
        })
        .collect();
    Ok(result)
}

#[tauri::command]
pub async fn ai_get_providers(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderListItem>, CommandError> {
    ai_get_providers_impl(&state).await
}

#[tauri::command]
pub async fn ai_fetch_remote_models(
    protocol: String,
    endpoint: String,
    api_key: String,
) -> Result<Vec<ModelInfo>, CommandError> {
    let proto = match protocol.as_str() {
        "open_ai_compatible" => crate::ai::custom::CustomProtocol::OpenAiCompatible,
        "open_ai_responses" => crate::ai::custom::CustomProtocol::OpenAiResponses,
        "anthropic_compatible" => crate::ai::custom::CustomProtocol::AnthropicCompatible,
        other => {
            return Err(CommandError::Validation(format!(
                "Unknown protocol: {other}"
            )))
        }
    };

    crate::ai::custom::fetch_remote_models(proto, &endpoint, &api_key)
        .await
        .cmd_err("ai_fetch_remote_models")
}

pub(crate) async fn ai_validate_config_impl(
    state: &AppState,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    state.ensure_ai_ready().await;
    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| {
            CommandError::NotFound(format!("Provider {:?} not found", config.provider_type))
        })?;
    provider
        .validate_config(&config)
        .await
        .cmd_err("ai_validate_config")
}

#[tauri::command]
pub async fn ai_validate_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    ai_validate_config_impl(&state, config).await
}

pub(crate) async fn ai_save_config_impl(
    state: &AppState,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    state.ensure_ai_ready().await;
    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| {
            CommandError::NotFound(format!("Provider {:?} not found", config.provider_type))
        })?;

    provider
        .initialize(&config)
        .await
        .cmd_err("ai_save_config")?;

    state
        .store
        .save_ai_config(&config)
        .await
        .cmd_err("ai_save_config")
}

#[tauri::command]
pub async fn ai_save_config(
    handle: AppHandle,
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    ai_save_config_impl(&state, config).await?;
    let _ = handle.emit("ai:config-changed", true);
    Ok(())
}

pub(crate) async fn ai_get_config_impl(
    state: &AppState,
) -> Result<Option<AiProviderConfig>, CommandError> {
    Ok(state.store.get_ai_config().await)
}

#[tauri::command]
pub async fn ai_get_config(
    state: State<'_, AppState>,
) -> Result<Option<AiProviderConfig>, CommandError> {
    ai_get_config_impl(&state).await
}

pub(crate) async fn ai_delete_config_impl(state: &AppState) -> Result<(), CommandError> {
    for provider in state.ai_registry.all_providers().await {
        provider.reset().await;
    }
    state
        .store
        .delete_ai_config()
        .await
        .cmd_err("ai_delete_config")
}

#[tauri::command]
pub async fn ai_delete_config(
    handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    ai_delete_config_impl(&state).await?;
    let _ = handle.emit("ai:config-changed", false);
    Ok(())
}
