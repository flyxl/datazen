//! Safety boundary for caller-provided text used by AI prompts.
//!
//! AI command handlers must not trust a UI-side redaction step.  Keep this
//! module independent from Tauri and the provider implementations so every
//! command can sanitize text immediately before it becomes prompt content.
//!
//! All redaction goes through [`redact_for_gate`], the single public entry
//! point that accepts an [`AiSafetyGateConfig`].  Legacy helpers are removed;
//! callers must obtain the gate from the active model profile.

use regex::Regex;
use serde_json::{Map, Value};
use std::sync::OnceLock;

use datazen_modules::ai::{AiDataEgressLevel, AiSafetyGateConfig};

const MAX_AI_TEXT_BYTES: usize = 4_000;
pub const MAX_JSON_DEPTH: usize = 4;
pub const MAX_JSON_ARRAY_ITEMS: usize = 100;
pub const MAX_JSON_OBJECT_KEYS: usize = 100;

const SECRET_WORDS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "secret",
    "token",
    "authorization",
    "bearer",
    "credential",
    "credentials",
    "passphrase",
    "key",
];

/// Words that contain a SECRET_WORDS substring but must NOT be flagged as
/// sensitive (e.g. "keyboard" contains "key", "monkey" contains "key").
const SENSITIVE_WHITELIST: &[&str] = &["keyboard", "monkey", "donkey", "turnkey", "hockey"];

/// Regex for result-key prefix matching (canonical form after `key_words` join).
fn result_key_prefix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(results?_?|rows?|records?|data|payload|samples?_?|queryresults?|rawoutput|executionoutput)",
        )
        .expect("result key prefix regex")
    })
}

fn sensitive_assignment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)(^|[^\w])(?:(?:\\)?["'])?([A-Za-z][\w.-]*)(?:(?:\\)?["'])?\s*(?:\\?:|=)\s*"#,
        )
        .expect("sensitive assignment regex")
    })
}

fn uri_credentials_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)([a-z][a-z0-9+.-]*://)([^/@\s]+)@").expect("URI credentials regex")
    })
}

fn bearer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(bearer\s+)\S+").expect("Bearer token regex"))
}

fn query_secret_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)([?&](?:password|passwd|pwd|token|access_token|api_token|auth_token|oauth_token|api_key|apikey|secret|key)=)([^&\s]+)",
        )
        .expect("query secret regex")
    })
}

/// Split an identifier or key into camelCase/snake_case word tokens (all lowercase).
fn key_words(key: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_is_lower_or_digit = false;

    for ch in key.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && previous_is_lower_or_digit && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(ch.to_ascii_lowercase());
            previous_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            previous_is_lower_or_digit = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Check whether a key looks like it holds a secret/credential value.
///
/// Token-boundary matching: a word must match exactly (not as a substring of a
/// larger word).  The whitelist exempts common words that happen to contain a
/// sensitive token (e.g. "keyboard", "monkey", "turnkey").
fn is_sensitive_key(key: &str) -> bool {
    let words = key_words(key);

    // Whitelist: if the canonical form is in the whitelist, never flag it.
    let canonical = words.join("");
    if SENSITIVE_WHITELIST.contains(&canonical.as_str()) {
        return false;
    }

    words
        .iter()
        .any(|word| SECRET_WORDS.iter().any(|secret| word == secret))
        || matches!(
            canonical.as_str(),
            "apitoken"
                | "authtoken"
                | "oauthtoken"
                | "accesstoken"
                | "refreshtoken"
                | "clienttoken"
                | "privatekey"
                | "encryptionkey"
                | "signingkey"
                | "apikey"
                | "apisecret"
                | "clientsecret"
                | "passphrase"
        )
}

/// Check whether a key represents a result/data payload container.
///
/// Uses regex prefix matching on the canonical form for broader coverage.
fn is_result_key(key: &str) -> bool {
    let canonical = key_words(key).join("");
    result_key_prefix_re().is_match(&canonical)
}

fn consume_assigned_value(value: &str, start: usize) -> usize {
    // BUG-14: Use safe `.or(0)` instead of `.expect()` on production paths.
    let mut index = start;
    while index < value.len() {
        let Some(ch) = value[index..].chars().next() else {
            break;
        };
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }

    let first = value[index..].chars().next();
    let (quote, quoted_start) = match first {
        Some('\\') if matches!(value[index..].chars().nth(1), Some('"' | '\'')) => {
            (value[index..].chars().nth(1), index + 2)
        }
        Some('"' | '\'' | '`') => (first, index + 1),
        _ => (None, index),
    };

    if let Some(quote) = quote {
        let mut cursor = quoted_start;
        while cursor < value.len() {
            let Some(ch) = value[cursor..].chars().next() else {
                break;
            };
            if ch == '\\' {
                cursor += ch.len_utf8();
                if cursor < value.len() {
                    if let Some(escaped) = value[cursor..].chars().next() {
                        cursor += escaped.len_utf8();
                    }
                }
            } else {
                cursor += ch.len_utf8();
                if ch == quote {
                    return cursor;
                }
            }
        }
        return cursor;
    }

    let mut cursor = quoted_start;
    while cursor < value.len() {
        let Some(ch) = value[cursor..].chars().next() else {
            break;
        };
        if ch.is_whitespace() || ",;)]}".contains(ch) {
            break;
        }
        cursor += ch.len_utf8();
    }
    cursor
}

fn redact_sensitive_assignments(value: &str) -> String {
    let mut result = String::new();
    let mut cursor = 0;
    let mut search_start = 0;

    while search_start < value.len() {
        let Some(captures) = sensitive_assignment_re().captures(&value[search_start..]) else {
            break;
        };
        let Some(full) = captures.get(0) else {
            break;
        };
        let Some(key) = captures.get(2) else { break };
        let match_start = search_start + full.start();
        if !is_sensitive_key(key.as_str()) {
            let advance = value[match_start..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
            search_start = match_start + advance;
            continue;
        }

        let prefix_len = captures
            .get(1)
            .map(|prefix| prefix.as_str().len())
            .unwrap_or(0);
        let key_start = match_start + prefix_len;
        if key_start < cursor {
            search_start = match_start + full.as_str().len();
            continue;
        }
        let full_end = search_start + full.end();
        let value_end = consume_assigned_value(value, full_end);
        result.push_str(&value[cursor..key_start]);
        result.push_str("[REDACTED]");
        cursor = value_end;
        search_start = value_end;
    }

    result.push_str(&value[cursor..]);
    result
}

fn redact_plain_text(value: &str) -> String {
    let uri_redacted = uri_credentials_re().replace_all(value, "${1}[REDACTED]@");
    let bearer_redacted = bearer_re().replace_all(&uri_redacted, "${1}[REDACTED]");
    let query_redacted = query_secret_re().replace_all(&bearer_redacted, "${1}[REDACTED]");
    redact_sensitive_assignments(&query_redacted)
}

fn sanitize_json_with_gate(value: Value, depth: usize, gate: &AiSafetyGateConfig) -> Value {
    if depth >= MAX_JSON_DEPTH && value.is_object() {
        return Value::String("[truncated]".into());
    }

    match value {
        Value::String(s) => Value::String(redact_plain_text(&s)),
        Value::Array(values) => {
            let max_items = match gate.data_egress_level {
                AiDataEgressLevel::Strict => 0,
                AiDataEgressLevel::SampleMasked => {
                    (gate.max_sample_rows as usize).min(MAX_JSON_ARRAY_ITEMS)
                }
                AiDataEgressLevel::Unrestricted => MAX_JSON_ARRAY_ITEMS,
            };
            Value::Array(
                values
                    .into_iter()
                    .take(max_items)
                    .map(|value| sanitize_json_with_gate(value, depth + 1, gate))
                    .collect(),
            )
        }
        Value::Object(map) => {
            let mut sanitized = Map::new();
            for (key, value) in map.into_iter().take(MAX_JSON_OBJECT_KEYS) {
                if is_sensitive_key(&key) {
                    continue;
                }
                if gate.data_egress_level == AiDataEgressLevel::Strict && is_result_key(&key) {
                    continue;
                }
                sanitized.insert(key, sanitize_json_with_gate(value, depth + 1, gate));
            }
            Value::Object(sanitized)
        }
        other => other,
    }
}

fn redact_json_text_with_gate(value: &str, gate: &AiSafetyGateConfig) -> Option<String> {
    let trimmed = value.trim();
    if !((trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('"') && trimmed.ends_with('"')))
    {
        return None;
    }

    serde_json::from_str::<Value>(trimmed)
        .ok()
        .and_then(|parsed| serde_json::to_string(&sanitize_json_with_gate(parsed, 0, gate)).ok())
}

fn truncate_to_bytes(value: String, limit: usize) -> String {
    if value.len() <= limit {
        return value;
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

/// Sanitize caller-provided text before it becomes AI prompt content.
///
/// This is the **single public entry point**.  Credentials are always redacted.
/// Result/payload row stripping and array limits are governed by the gate
/// configuration from the active model profile.
pub fn redact_for_gate(value: &str, gate: &AiSafetyGateConfig) -> String {
    let limit = if gate.max_context_bytes > 0 {
        gate.max_context_bytes
    } else {
        MAX_AI_TEXT_BYTES
    };
    let text = redact_json_text_with_gate(value, gate).unwrap_or_else(|| redact_plain_text(value));
    truncate_to_bytes(text, limit)
}

/// Global execution policy can tighten a model profile's egress level.
pub fn effective_safety_gate(
    settings: &datazen_modules::ai::AiSettingsConfig,
    policy: &datazen_modules::settings::ExecutionSettings,
) -> AiSafetyGateConfig {
    let mut gate = settings
        .selected_profile()
        .map(|profile| profile.safety_gate.clone())
        .unwrap_or_else(|| AiSafetyGateConfig {
            data_egress_level: AiDataEgressLevel::Unrestricted,
            ..AiSafetyGateConfig::default()
        });
    if policy.ai_strict_egress {
        gate.data_egress_level = AiDataEgressLevel::Strict;
    }
    gate
}

#[cfg(test)]
#[path = "safety_tests.rs"]
mod tests;
