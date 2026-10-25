//! Shared helpers for AI command handlers.

use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen_ai_providers::AiProviderConfig;
use datazen_ai_providers::*;
use datazen_platform_api::dto::session::SessionView;
use datazen_runtime::application::{LiveConnectionLease, LivePurpose};
use std::sync::Arc;
use tauri::{Emitter, WebviewWindow};

pub(crate) use datazen_feature_core::ai::response::language_hint;

/// Delivers streaming chunks to the UI (or test collector).
pub(crate) type StreamCallback = Arc<dyn Fn(&str, Result<StreamChunk, AiError>) + Send + Sync>;

pub(crate) fn window_stream_callback(window: &WebviewWindow) -> StreamCallback {
    let window = window.clone();
    Arc::new(move |request_id, result| {
        emit_stream_chunk_or_error(&window, request_id, result);
    })
}

/// Resolve the active model profile's safety gate configuration.
///
/// Falls back to a relaxed gate (credentials only) when no profile is found,
/// matching the previous `ai_strict_egress = false` default behavior.
pub(crate) async fn resolve_safety_gate(state: &AppState) -> AiSafetyGateConfig {
    datazen_feature_core::ai::safety::effective_safety_gate(
        &state.store.get_ai_settings_config().await,
        &state.store.execution_settings().await,
    )
}

pub(crate) async fn resolve_ai(
    state: &AppState,
) -> Result<(Arc<dyn AiProvider>, AiProviderConfig), CommandError> {
    state.ensure_ai_ready().await;

    let config = state
        .store
        .get_ai_config()
        .await
        .ok_or_else(|| CommandError::NotConfigured("AI_NOT_CONFIGURED".into()))?;

    let safe_endpoint = config
        .endpoint
        .as_deref()
        .map(datazen_platform_infra::redaction::redact_url_for_log);
    tracing::debug!(
        provider = %config.provider_type,
        model = %config.model,
        endpoint = ?safe_endpoint,
        "resolve_ai: provider config"
    );

    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| CommandError::NotConfigured("AI_PROVIDER_NOT_AVAILABLE".into()))?;

    Ok((provider, config))
}

pub(crate) async fn build_connections_context(state: &AppState, lang: &str) -> String {
    datazen_feature_core::ai::response::build_connections_context(state.store.as_ref(), lang).await
}

/// Resolve an AI command's runtime session and borrow its already-open driver
/// connection. The IPC contract remains a scalar `dbSessionId`; the Runtime
/// enforces ownership, epoch validation, and physical-resource lifetime.
pub(crate) async fn borrow_runtime_session(
    state: &AppState,
    db_session_id: &str,
    command: &str,
) -> Result<(SessionView, LiveConnectionLease), CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let call = adapter.begin_call();
    adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?
        .borrow_live_connection_by_id(call.context(), db_session_id, LivePurpose::Inspection)
        .await
        .map_err(crate::platform::into_command_error)
        .map_err(|error| match error {
            CommandError::NotFound(message) => {
                CommandError::NotFound(format!("{command}: {message}"))
            }
            other => other,
        })
}

/// Resolve only the runtime session view when an AI operation needs owner or
/// namespace metadata but does not execute a database command.
pub(crate) async fn runtime_session(
    state: &AppState,
    db_session_id: &str,
    command: &str,
) -> Result<SessionView, CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let call = adapter.begin_call();
    adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?
        .get_session_by_id(call.context(), db_session_id)
        .await
        .map_err(crate::platform::into_command_error)
        .map_err(|error| match error {
            CommandError::NotFound(message) => {
                CommandError::NotFound(format!("{command}: {message}"))
            }
            other => other,
        })
}

pub(crate) fn parse_ai_json<T: serde::de::DeserializeOwned>(
    raw: &str,
    finish_reason: Option<&str>,
    cmd: &str,
) -> Result<T, CommandError> {
    datazen_feature_core::ai::response::parse_ai_json(raw, finish_reason, cmd).map_err(Into::into)
}

pub(crate) fn emit_stream_chunk_or_error<R: tauri::Runtime>(
    emitter: &impl Emitter<R>,
    request_id: &str,
    result: Result<StreamChunk, AiError>,
) {
    match result {
        Ok(chunk) => {
            let mut payload = serde_json::json!({
                "requestId": request_id,
                "content": chunk.content,
                "done": chunk.done,
                "usage": chunk.usage,
            });
            if let Some(reasoning) = &chunk.reasoning {
                payload["reasoning"] = serde_json::Value::String(reasoning.clone());
            }
            if let Some(tool_calls) = &chunk.tool_calls {
                payload["toolCalls"] = serde_json::to_value(tool_calls).unwrap_or_default();
            }
            let _ = emitter.emit("ai:stream-chunk", payload);
        }
        Err(e) => {
            let _ = emitter.emit(
                "ai:stream-error",
                serde_json::json!({
                    "requestId": request_id,
                    "error": e.to_string(),
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_feature_core::ai::response::{
        find_first_balanced_json, repair_json, strip_fences_all,
    };

    // ── find_first_balanced_json ──

    #[test]
    fn find_json_simple_object() {
        let input = r#"{"key": "value"}"#;
        let result = find_first_balanced_json(input).unwrap();
        assert_eq!(result, r#"{"key": "value"}"#);
    }

    #[test]
    fn find_json_with_prefix_text() {
        let input = r#"Here is the result: {"key": "value"} and some trailing text"#;
        let result = find_first_balanced_json(input).unwrap();
        assert_eq!(result, r#"{"key": "value"}"#);
    }

    #[test]
    fn find_json_with_prefix_text_multiline() {
        let input = "Here is the result:\n```json\n{\"key\": \"value\"}\n```\nand trailing";
        let result = find_first_balanced_json(input).unwrap();
        assert_eq!(result, r#"{"key": "value"}"#);
    }

    #[test]
    fn find_json_array() {
        let input = r#"[1, 2, 3]"#;
        let result = find_first_balanced_json(input).unwrap();
        assert_eq!(result, r#"[1, 2, 3]"#);
    }

    #[test]
    fn find_json_nested() {
        let input = r#"{"a": {"b": [1, 2]}}"#;
        let result = find_first_balanced_json(input).unwrap();
        assert_eq!(result, r#"{"a": {"b": [1, 2]}}"#);
    }

    #[test]
    fn find_json_with_strings_containing_braces() {
        let input = r#"{"msg": "hello {world}", "n": 42}"#;
        let result = find_first_balanced_json(input).unwrap();
        assert_eq!(result, r#"{"msg": "hello {world}", "n": 42}"#);
    }

    #[test]
    fn find_json_returns_none_for_plain_text() {
        assert!(find_first_balanced_json("no json here").is_none());
    }

    // ── strip_fences_all ──

    #[test]
    fn strip_single_fence() {
        let input = "```json\n{\"a\": 1}\n```";
        let result = strip_fences_all(input);
        assert_eq!(result, r#"{"a": 1}"#);
    }

    #[test]
    fn strip_multiple_fences() {
        let input = "```json\n{\"a\": 1}\n```\nSome text\n```json\n{\"b\": 2}\n```";
        let result = strip_fences_all(input);
        assert!(result.contains(r#"{"a": 1}"#));
        assert!(result.contains(r#"{"b": 2}"#));
    }

    #[test]
    fn strip_fence_without_closing() {
        let input = "```json\n{\"a\": 1}\nno closing fence";
        let result = strip_fences_all(input);
        assert!(result.contains(r#"{"a": 1}"#));
    }

    #[test]
    fn strip_no_fence_passthrough() {
        let input = r#"{"a": 1}"#;
        let result = strip_fences_all(input);
        assert_eq!(result, r#"{"a": 1}"#);
    }

    // ── repair_json ──

    #[test]
    fn repair_trailing_comma_object() {
        let input = r#"{"a": 1, "b": 2,}"#;
        let repaired = repair_json(input).unwrap();
        assert_eq!(repaired, r#"{"a": 1, "b": 2}"#);
    }

    #[test]
    fn repair_trailing_comma_array() {
        let input = r#"[1, 2, 3,]"#;
        let repaired = repair_json(input).unwrap();
        assert_eq!(repaired, r#"[1, 2, 3]"#);
    }

    #[test]
    fn repair_no_change_needed() {
        let input = r#"{"a": 1}"#;
        assert!(repair_json(input).is_none());
    }

    // ── parse_ai_json integration ──

    #[test]
    fn parse_json_from_fenced_response() {
        let raw = "Sure! Here is the SQL:\n```json\n{\"sql\": \"SELECT 1\"}\n```";
        let result: serde_json::Value = parse_ai_json(raw, Some("stop"), "test").unwrap();
        assert_eq!(result["sql"], "SELECT 1");
    }

    #[test]
    fn parse_json_from_prefixed_response() {
        let raw = "Here is the result: {\"query\": \"hello\"}";
        let result: serde_json::Value = parse_ai_json(raw, Some("stop"), "test").unwrap();
        assert_eq!(result["query"], "hello");
    }

    #[test]
    fn parse_json_with_trailing_comma() {
        let raw = "```json\n{\"sql\": \"SELECT 1\",}\n```";
        let result: serde_json::Value = parse_ai_json(raw, Some("stop"), "test").unwrap();
        assert_eq!(result["sql"], "SELECT 1");
    }
}
