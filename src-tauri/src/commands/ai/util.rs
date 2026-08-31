//! Shared helpers for AI command handlers.

use crate::ai::safety::redact_for_ai;
use crate::ai::*;
use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen_ai_api::AiProviderConfig;
use std::sync::Arc;
use tauri::{Emitter, WebviewWindow};

pub(crate) fn language_hint(lang: &str) -> String {
    let lang_name = match lang {
        "zh-CN" => "Chinese (Simplified)",
        "zh-TW" => "Chinese (Traditional)",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        _ => lang,
    };
    format!("\n\nIMPORTANT: All free-text content in your response MUST be in {lang_name}.")
}

pub(crate) fn inject_language_hint(messages: &mut [ChatMessage], lang: &str) {
    if let Some(sys) = messages.iter_mut().find(|m| m.role == MessageRole::System) {
        sys.content.push_str(&language_hint(lang));
    }
}

/// Delivers streaming chunks to the UI (or test collector).
pub(crate) type StreamCallback = Arc<dyn Fn(&str, Result<StreamChunk, AiError>) + Send + Sync>;

pub(crate) fn window_stream_callback(window: &WebviewWindow) -> StreamCallback {
    let window = window.clone();
    Arc::new(move |request_id, result| {
        emit_stream_chunk_or_error(&window, request_id, result);
    })
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
        .map(crate::log_redact::redact_url_for_log);
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
    let conns = state.store.get_connections().await;
    if conns.is_empty() {
        return String::new();
    }
    let mut lines = Vec::new();
    let header = if lang.starts_with("zh") {
        "用户有以下可用的数据库连接："
    } else {
        "The user has the following database connections available:"
    };
    lines.push(header.to_string());
    for c in &conns {
        lines.push(format!(
            "- \"{}\" ({:?}) — id: {}",
            c.name, c.database_type, c.id
        ));
    }
    lines.join("\n")
}

pub(crate) fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub(crate) fn parse_ai_json<T: serde::de::DeserializeOwned>(
    raw: &str,
    finish_reason: Option<&str>,
    cmd: &str,
) -> Result<T, CommandError> {
    let content = strip_markdown_fences(raw);
    if content.trim().is_empty() {
        tracing::error!(cmd, "LLM returned empty response");
        return Err(CommandError::Internal("LLM returned empty response".into()));
    }

    if let Ok(val) = serde_json::from_str::<T>(&content) {
        return Ok(val);
    }

    if let Some(extracted) = extract_json_boundary(&content) {
        if let Ok(val) = serde_json::from_str::<T>(extracted) {
            tracing::debug!(cmd, "Parsed JSON after extracting from mixed content");
            return Ok(val);
        }
    }

    let Err(err) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Err(CommandError::Internal(
            "AI response JSON structure does not match the expected schema.".into(),
        ));
    };
    tracing::error!(
        cmd,
        content_len = content.len(),
        content_redacted = redact_for_ai(&content) != content,
        ?finish_reason,
        "JSON parse failed: {err}"
    );
    let is_truncated = matches!(finish_reason, Some("length") | Some("max_tokens"));
    if is_truncated {
        Err(CommandError::Internal(
            "AI response was truncated due to max_tokens limit. \
             Please increase the \"Max Tokens\" setting in AI configuration."
                .into(),
        ))
    } else {
        Err(CommandError::Internal(format!(
            "Failed to parse AI response. The model may have returned an invalid format. \
             Try again or increase Max Tokens in settings. (detail: {err})"
        )))
    }
}

/// Extract the first complete JSON object `{...}` or array `[...]` from text
/// that may contain trailing non-JSON content (e.g. model reasoning).
pub(crate) fn extract_json_boundary(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    let (open, close) = if trimmed.starts_with('{') {
        ('{', '}')
    } else if trimmed.starts_with('[') {
        ('[', ']')
    } else {
        return None;
    };

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in trimmed.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(&trimmed[..=i]);
            }
        }
    }
    None
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

pub(crate) fn strip_markdown_fences(s: &str) -> String {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let body = rest
            .strip_prefix("json")
            .or_else(|| rest.strip_prefix("JSON"))
            .unwrap_or(rest);
        if let Some(end) = body.rfind("```") {
            return body[..end].trim().to_string();
        }
    }
    trimmed.to_string()
}
