//! Shared helpers for AI command handlers.

use crate::ai::safety::redact_for_gate;
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
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "ru" => "Russian",
        "pt-BR" => "Portuguese (Brazil)",
        other if other.starts_with("zh") => "Chinese (Simplified)",
        other if other.starts_with("en") => "English",
        _ => lang,
    };
    format!(
        "\n\nIMPORTANT LANGUAGE REQUIREMENT:\nThe application's active display language is set to {lang_name}.\nRegardless of the language used in system instructions or user input, all your natural language responses, explanations, summaries, descriptions, and free-text fields (including string values in JSON responses) MUST be written in {lang_name}. Technical identifiers, SQL syntax, and JSON keys must remain unchanged."
    )
}

pub(crate) fn inject_language_hint(messages: &mut [ChatMessage], lang: &str) {
    if let Some(sys) = messages.iter_mut().find(|m| m.role == MessageRole::System) {
        if !sys.content.contains("IMPORTANT LANGUAGE REQUIREMENT") {
            sys.content.push_str(&language_hint(lang));
        }
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

/// Resolve the active model profile's safety gate configuration.
///
/// Falls back to a relaxed gate (credentials only) when no profile is found,
/// matching the previous `ai_strict_egress = false` default behavior.
pub(crate) async fn resolve_safety_gate(state: &AppState) -> AiSafetyGateConfig {
    let settings = state.store.get_ai_settings_config().await;
    settings
        .profiles
        .iter()
        .find(|p| p.id == settings.active_profile_id)
        .or_else(|| settings.profiles.iter().find(|p| p.is_default))
        .or_else(|| settings.profiles.first())
        .map(|p| p.safety_gate.clone())
        .unwrap_or_else(|| AiSafetyGateConfig {
            data_egress_level: AiDataEgressLevel::Unrestricted,
            ..AiSafetyGateConfig::default()
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

#[allow(dead_code)]
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
    // Step 1: Strip all markdown fences
    let content = strip_fences_all(raw);
    if content.trim().is_empty() {
        tracing::error!(cmd, "LLM returned empty response");
        return Err(CommandError::Internal("LLM returned empty response".into()));
    }

    // Step 2: Try direct parse
    if let Ok(val) = serde_json::from_str::<T>(&content) {
        return Ok(val);
    }

    // Step 3: Try balanced JSON extraction
    if let Some(extracted) = find_first_balanced_json(&content) {
        if let Ok(val) = serde_json::from_str::<T>(extracted) {
            tracing::debug!(cmd, "Parsed JSON via balanced extraction");
            return Ok(val);
        }
        // Step 3b: Try repairing common JSON issues (trailing commas, etc.)
        if let Some(repaired) = repair_json(extracted) {
            if let Ok(val) = serde_json::from_str::<T>(&repaired) {
                tracing::debug!(cmd, "Parsed JSON after repair");
                return Ok(val);
            }
        }
    }

    // Step 4: Try legacy boundary extraction (backward compat)
    if let Some(extracted) = extract_json_boundary(&content) {
        if let Ok(val) = serde_json::from_str::<T>(extracted) {
            tracing::debug!(cmd, "Parsed JSON via legacy boundary extraction");
            return Ok(val);
        }
    }

    // Step 5: All parse attempts failed — diagnostic error
    let Err(err) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Err(CommandError::Internal(
            "AI response JSON structure does not match the expected schema.".into(),
        ));
    };
    tracing::error!(
        cmd,
        content_len = content.len(),
        content_redacted = redact_for_gate(&content, &AiSafetyGateConfig::default()) != content,
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

/// Attempt to repair common JSON formatting issues from LLM output.
///
/// Currently handles: trailing commas before `}` or `]`.
fn repair_json(s: &str) -> Option<String> {
    let mut result = s.to_string();
    let mut changed = false;

    // Remove trailing commas before closing braces/brackets
    // Use a simple byte-scan approach
    let bytes = result.as_bytes();
    let mut fixed = Vec::with_capacity(bytes.len());
    let mut in_string = false;
    let mut escape = false;

    for i in 0..bytes.len() {
        let ch = bytes[i];
        if escape {
            escape = false;
            fixed.push(ch);
            continue;
        }
        if ch == b'\\' && in_string {
            escape = true;
            fixed.push(ch);
            continue;
        }
        if ch == b'"' {
            in_string = !in_string;
            fixed.push(ch);
            continue;
        }
        if in_string {
            fixed.push(ch);
            continue;
        }
        // Check for trailing comma: comma followed by optional whitespace and then `}` or `]`
        if ch == b',' {
            let rest = &s[i + 1..];
            let trimmed = rest.trim_start();
            if trimmed.starts_with('}') || trimmed.starts_with(']') {
                changed = true;
                // Skip the comma
                continue;
            }
        }
        fixed.push(ch);
    }

    if changed {
        result = String::from_utf8(fixed).ok()?;
        Some(result)
    } else {
        None
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

/// Strip ALL markdown fence blocks from text, returning only the inner content.
///
/// Handles multiple fenced blocks and mixed content (text between fences).
pub(crate) fn strip_fences_all(s: &str) -> String {
    let mut result = String::new();
    let mut remaining = s.trim();

    while !remaining.is_empty() {
        if let Some(fence_start) = remaining.find("```") {
            // Append any text before the fence
            result.push_str(&remaining[..fence_start]);
            let after_fence = &remaining[fence_start + 3..];
            // Skip language tag line
            if let Some(newline) = after_fence.find('\n') {
                let after_lang = &after_fence[newline + 1..];
                // Find closing fence
                if let Some(close) = after_lang.find("```") {
                    result.push_str(after_lang[..close].trim());
                    remaining = after_fence[newline + 1 + close + 3..].trim_start();
                } else {
                    // No closing fence — take the rest as content
                    result.push_str(after_lang.trim());
                    break;
                }
            } else {
                // Single line with ``` — treat as plain text
                result.push_str("```");
                break;
            }
        } else {
            result.push_str(remaining);
            break;
        }
    }

    result.trim().to_string()
}

/// Find the first balanced JSON object `{...}` or array `[...]` in arbitrary text.
///
/// This scans for the first `{` or `[` character, then walks the text tracking
/// brace/bracket depth while respecting string boundaries, to locate the
/// complete JSON structure.  Returns the extracted JSON slice, or `None` if
/// no balanced JSON is found.
pub(crate) fn find_first_balanced_json(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let len = bytes.len();

    for start in 0..len {
        let open = match bytes[start] {
            b'{' => b'}',
            b'[' => b']',
            _ => continue,
        };
        let mut depth = 1i32;
        let mut in_string = false;
        let mut escape = false;

        for i in (start + 1)..len {
            let ch = bytes[i];
            if escape {
                escape = false;
                continue;
            }
            if ch == b'\\' && in_string {
                escape = true;
                continue;
            }
            if ch == b'"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            match ch {
                b if b == bytes[start] => depth += 1,
                b if b == open => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&s[start..=i]);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Emit a task progress event to the frontend.
///
/// Used by non-streaming commands to report progress during long-running operations.
pub(crate) fn emit_task_progress<R: tauri::Runtime>(
    emitter: &impl Emitter<R>,
    request_id: &str,
    phase: &str,
    message: &str,
) {
    let _ = emitter.emit(
        "ai:task-progress",
        serde_json::json!({
            "requestId": request_id,
            "phase": phase,
            "message": message,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
