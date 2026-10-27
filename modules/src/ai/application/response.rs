//! AI 响应解析与提示词文本处理（自 modules::ai::response 归位，D6 item 4 切片三）。
//!
//! 纯规则：语言提示注入、连接上下文文案、JSON 提取/修复/围栏剥离。错误经
//! [`AiError::Validation`]（裸消息，display 与宿主侧 FeatureError::Validation/
//! Internal 的 "{0}" 裸文案逐字兼容）返回。

use crate::ai::domain::safety::redact_for_gate;
use crate::ai::domain::types::{
    AiError as CommandError, AiSafetyGateConfig, ChatMessage, MessageRole,
};
use crate::connection::domain::{ConnectionRepository, ConnectionScope};
pub fn language_hint(lang: &str) -> String {
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

pub fn inject_language_hint(messages: &mut [ChatMessage], lang: &str) {
    if let Some(sys) = messages.iter_mut().find(|m| m.role == MessageRole::System) {
        if !sys.content.contains("IMPORTANT LANGUAGE REQUIREMENT") {
            sys.content.push_str(&language_hint(lang));
        }
    }
}

pub async fn build_connections_context(
    catalog: &dyn ConnectionRepository,
    scope: &ConnectionScope,
    lang: &str,
) -> String {
    let conns = catalog.list(scope).await.unwrap_or_default();
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
pub fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub fn parse_ai_json<T: serde::de::DeserializeOwned>(
    raw: &str,
    finish_reason: Option<&str>,
    cmd: &str,
) -> Result<T, CommandError> {
    // Step 1: Strip all markdown fences
    let content = strip_fences_all(raw);
    if content.trim().is_empty() {
        tracing::error!(cmd, "LLM returned empty response");
        return Err(CommandError::Validation(
            "LLM returned empty response".into(),
        ));
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
        return Err(CommandError::Validation(
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
        Err(CommandError::Validation(
            "AI response was truncated due to max_tokens limit. \
             Please increase the \"Max Tokens\" setting in AI configuration."
                .into(),
        ))
    } else {
        Err(CommandError::Validation(format!(
            "Failed to parse AI response. The model may have returned an invalid format. \
             Try again or increase Max Tokens in settings. (detail: {err})"
        )))
    }
}

/// Attempt to repair common JSON formatting issues from LLM output.
///
/// Currently handles: trailing commas before `}` or `]`.
pub fn repair_json(s: &str) -> Option<String> {
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
pub fn extract_json_boundary(s: &str) -> Option<&str> {
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

pub fn strip_markdown_fences(s: &str) -> String {
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
pub fn strip_fences_all(s: &str) -> String {
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
pub fn find_first_balanced_json(s: &str) -> Option<&str> {
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
