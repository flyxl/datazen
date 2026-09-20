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

use crate::ai::{AiDataEgressLevel, AiSafetyGateConfig};

const MAX_AI_TEXT_BYTES: usize = 4_000;
pub(crate) const MAX_JSON_DEPTH: usize = 4;
pub(crate) const MAX_JSON_ARRAY_ITEMS: usize = 100;
pub(crate) const MAX_JSON_OBJECT_KEYS: usize = 100;

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
pub(crate) fn redact_for_gate(value: &str, gate: &AiSafetyGateConfig) -> String {
    let limit = if gate.max_context_bytes > 0 {
        gate.max_context_bytes
    } else {
        MAX_AI_TEXT_BYTES
    };
    let text = redact_json_text_with_gate(value, gate).unwrap_or_else(|| redact_plain_text(value));
    truncate_to_bytes(text, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: default strict gate (redact credentials + strip result rows).
    fn strict_gate() -> AiSafetyGateConfig {
        AiSafetyGateConfig {
            data_egress_level: AiDataEgressLevel::Strict,
            ..Default::default()
        }
    }

    /// Helper: relaxed gate (redact credentials but keep result rows).
    fn relaxed_gate() -> AiSafetyGateConfig {
        AiSafetyGateConfig {
            data_egress_level: AiDataEgressLevel::Unrestricted,
            ..Default::default()
        }
    }

    /// Helper: sample-masked gate with custom row limit.
    fn sample_gate(max_rows: u32) -> AiSafetyGateConfig {
        AiSafetyGateConfig {
            data_egress_level: AiDataEgressLevel::SampleMasked,
            max_sample_rows: max_rows,
            ..Default::default()
        }
    }

    // ── basic credential redaction ──

    #[test]
    fn redacts_sensitive_key_assignments() {
        let gate = strict_gate();
        for (input, secret) in [
            ("password=db-secret", "db-secret"),
            ("apiKey=sk-live-123", "sk-live-123"),
            ("Authorization: Bearer bearer-secret", "bearer-secret"),
        ] {
            let output = redact_for_gate(input, &gate);
            assert!(!output.contains(secret), "{output}");
            assert!(output.contains("[REDACTED]"), "{output}");
        }

        let json_output = redact_for_gate("{\"clientSecret\":\"top-secret\"}", &gate);
        assert!(!json_output.contains("top-secret"), "{json_output}");
        assert_eq!(json_output, "{}");
    }

    #[test]
    fn redacts_uri_credentials_and_query_secrets() {
        let gate = strict_gate();
        let input =
            "url=mysql://user:uri-secret@db.example/app?token=query-secret&api_key=key-secret";
        let output = redact_for_gate(input, &gate);
        for secret in ["uri-secret", "query-secret", "key-secret"] {
            assert!(!output.contains(secret), "{output}");
        }
        assert!(
            output.contains("mysql://[REDACTED]@db.example/app"),
            "{output}"
        );
    }

    #[test]
    fn redacts_bearer_tokens_case_insensitively() {
        let gate = strict_gate();
        let output = redact_for_gate("Authorization: bearer abc.def.ghi", &gate);
        assert!(!output.contains("abc.def.ghi"), "{output}");
        assert!(output.contains("[REDACTED]"), "{output}");

        let standalone = redact_for_gate("prefix bearer abc.def.ghi", &gate);
        assert!(!standalone.contains("abc.def.ghi"), "{standalone}");
        assert!(standalone.contains("bearer [REDACTED]"), "{standalone}");
    }

    #[test]
    fn redacts_sensitive_assignments_and_connection_credentials() {
        let gate = strict_gate();
        let input = "query failed: apiToken=prefix-token; SELECT * FROM t WHERE password = 'db-secret' AND apiToken=token-secret; \
            url=mysql://user:uri-secret@db.example/app?token=query-secret; Bearer bearer-secret";
        let output = redact_for_gate(input, &gate);

        for secret in [
            "db-secret",
            "token-secret",
            "prefix-token",
            "uri-secret",
            "query-secret",
            "bearer-secret",
        ] {
            assert!(!output.contains(secret), "{output}");
        }
        assert!(output.contains("[REDACTED]"), "{output}");
    }

    // ── JSON structure redaction ──

    #[test]
    fn removes_nested_json_secrets_and_result_rows_in_strict_mode() {
        let input = serde_json::json!({
            "apiToken": "API_TOKEN_HEAD\"TAIL\\END",
            "nested": {"password": "PASSWORD_SECRET", "safe": "keep"},
            "resultRows": [{"email": "user@example.test"}],
        })
        .to_string();
        let output = redact_for_gate(&input, &strict_gate());
        let parsed: Value = serde_json::from_str(&output).expect("safe JSON");

        assert_eq!(parsed, serde_json::json!({"nested": {"safe": "keep"}}));
        for secret in [
            "API_TOKEN_HEAD",
            "TAIL",
            "END",
            "PASSWORD_SECRET",
            "user@example.test",
        ] {
            assert!(!output.contains(secret), "{output}");
        }
    }

    #[test]
    fn relaxed_egress_keeps_result_rows_but_still_redacts_secrets() {
        let input = serde_json::json!({
            "password": "PASSWORD_SECRET",
            "rows": [{"email": "user@example.test"}],
        })
        .to_string();
        let output = redact_for_gate(&input, &relaxed_gate());
        let parsed: Value = serde_json::from_str(&output).expect("safe JSON");

        assert_eq!(
            parsed,
            serde_json::json!({"rows": [{"email": "user@example.test"}]})
        );
        assert!(!output.contains("PASSWORD_SECRET"), "{output}");
    }

    #[test]
    fn bounds_large_prompt_text_without_panicking_on_utf8() {
        let gate = strict_gate();
        let output = redact_for_gate(&"表".repeat(3_000), &gate);
        assert!(output.len() <= gate.max_context_bytes);
        assert!(output.is_char_boundary(output.len()));
    }

    // ── whitelist: sensitive-key exemption ──

    #[test]
    fn whitelist_keyboard_not_flagged() {
        let gate = strict_gate();
        let input = serde_json::json!({
            "keyboard": "mechanical-switches",
        })
        .to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        // "keyboard" is whitelisted — should NOT be redacted
        assert_eq!(parsed["keyboard"], "mechanical-switches");
    }

    #[test]
    fn whitelist_monkey_not_flagged() {
        let gate = strict_gate();
        let input = serde_json::json!({
            "monkey": "test-value",
        })
        .to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(parsed["monkey"], "test-value");
    }

    #[test]
    fn whitelist_donkey_not_flagged() {
        let gate = strict_gate();
        let input = serde_json::json!({
            "donkey": "animal",
        })
        .to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(parsed["donkey"], "animal");
    }

    #[test]
    fn whitelist_turnkey_not_flagged() {
        let gate = strict_gate();
        let input = serde_json::json!({
            "turnkey": "solution",
        })
        .to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(parsed["turnkey"], "solution");
    }

    #[test]
    fn whitelist_hockey_not_flagged() {
        let gate = strict_gate();
        let input = serde_json::json!({
            "hockey": "sport",
        })
        .to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(parsed["hockey"], "sport");
    }

    #[test]
    fn non_whitelisted_key_containing_sensitive_word_is_still_flagged() {
        let gate = strict_gate();
        // "monkey_key" — underscore-separated, "key" token triggers
        let input = serde_json::json!({
            "monkey_key": "should-be-redacted",
        })
        .to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(
            parsed.get("monkey_key").is_none(),
            "monkey_key should be removed"
        );

        // "monkeyKey" — camelCase, "key" token triggers
        let input2 = serde_json::json!({
            "monkeyKey": "should-be-redacted",
        })
        .to_string();
        let output2 = redact_for_gate(&input2, &gate);
        let parsed2: Value = serde_json::from_str(&output2).expect("valid JSON");
        assert!(
            parsed2.get("monkeyKey").is_none(),
            "monkeyKey should be removed"
        );
    }

    // ── result-key prefix regex matching ──

    #[test]
    fn result_key_prefix_matches_common_patterns() {
        let gate = strict_gate();
        // "data" prefix
        let input = serde_json::json!({"data": [{"id": 1}]}).to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(
            parsed.get("data").is_none(),
            "data key should be removed in strict mode"
        );

        // "rows" prefix
        let input = serde_json::json!({"rows": [{"name": "Alice"}]}).to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(parsed.get("rows").is_none());

        // "resultRows" — canonical = "resultrows", matches "results?"
        let input = serde_json::json!({"resultRows": [{"x": 1}]}).to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(parsed.get("resultRows").is_none());

        // "payload" — direct match
        let input = serde_json::json!({"payload": {"secret": "val"}}).to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(parsed.get("payload").is_none());

        // "sampleData" — canonical = "sampledata", matches "samples?_?"
        let input = serde_json::json!({"sampleData": [{"id": 1}]}).to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(parsed.get("sampleData").is_none());
    }

    #[test]
    fn result_key_prefix_does_not_false_positive() {
        let gate = strict_gate();
        // "description" does NOT match any result-key prefix
        let input = serde_json::json!({"description": "a table of users"}).to_string();
        let output = redact_for_gate(&input, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(parsed["description"], "a table of users");
    }

    // ── deep nesting / truncation ──

    #[test]
    fn test_tester_nested_json_sensitive_keys_at_all_depths() {
        let input = serde_json::json!({
            "password": "root-secret",
            "level1": {
                "apiToken": "level1-secret",
                "level2": {
                    "clientSecret": "level2-secret",
                    "level3": {
                        "refreshToken": "level3-secret",
                        "safe": "keep-me"
                    }
                }
            }
        })
        .to_string();

        let output = redact_for_gate(&input, &strict_gate());
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(
            parsed,
            serde_json::json!({"level1": {"level2": {"level3": {"safe": "keep-me"}}}})
        );
        for secret in [
            "root-secret",
            "level1-secret",
            "level2-secret",
            "level3-secret",
        ] {
            assert!(!output.contains(secret), "{output}");
        }
    }

    #[test]
    fn test_tester_deep_json_truncates_objects_at_max_depth() {
        let input = serde_json::json!({
            "a": {"b": {"c": {"d": {"e": "too-deep", "safe": "visible"}}}}
        })
        .to_string();

        let output = redact_for_gate(&input, &relaxed_gate());
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(
            parsed,
            serde_json::json!({"a": {"b": {"c": {"d": "[truncated]"}}}})
        );
        assert!(!output.contains("too-deep"), "{output}");
    }

    #[test]
    fn test_tester_oversized_ascii_payload_truncates_at_byte_boundary() {
        let gate = AiSafetyGateConfig {
            max_context_bytes: MAX_AI_TEXT_BYTES,
            ..Default::default()
        };
        let input = "x".repeat(MAX_AI_TEXT_BYTES + 500);
        let output = redact_for_gate(&input, &gate);

        assert_eq!(output.len(), MAX_AI_TEXT_BYTES);
        assert!(output.is_char_boundary(output.len()));
        assert!(input.starts_with(&output));
    }

    #[test]
    fn test_tester_oversized_multibyte_payload_respects_char_boundaries() {
        let gate = AiSafetyGateConfig {
            max_context_bytes: MAX_AI_TEXT_BYTES,
            ..Default::default()
        };
        let unit = "表";
        let repeat = MAX_AI_TEXT_BYTES / unit.len() + 10;
        let input = unit.repeat(repeat);
        let output = redact_for_gate(&input, &gate);

        assert!(output.len() <= MAX_AI_TEXT_BYTES);
        assert!(output.is_char_boundary(output.len()));
        assert!(input.starts_with(&output));
        assert!(std::str::from_utf8(output.as_bytes()).is_ok());
    }

    #[test]
    fn test_tester_invalid_json_fallback_still_redacts_plain_secrets() {
        let gate = strict_gate();
        let input = r#"{"broken": true, password=leaked-secret"#;
        let output = redact_for_gate(input, &gate);

        assert!(!output.contains("leaked-secret"), "{output}");
        assert!(output.contains("[REDACTED]"), "{output}");
    }

    // ── egress policy simulation ──

    #[test]
    fn test_tester_strict_egress_strips_result_rows_like_chat_user_message() {
        let gate = strict_gate();
        let chat_payload = serde_json::json!({
            "question": "summarize",
            "queryResults": [{"id": 1, "email": "alice@corp.test"}],
            "password": "chat-secret"
        })
        .to_string();

        let output = redact_for_gate(&chat_payload, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(parsed, serde_json::json!({"question": "summarize"}));
        assert!(!output.contains("alice@corp.test"), "{output}");
        assert!(!output.contains("chat-secret"), "{output}");
    }

    #[test]
    fn test_tester_strict_egress_strips_tool_result_payload_like_chat_loop() {
        let gate = strict_gate();
        let tool_result = serde_json::json!({
            "rows": [{"name": "Bob"}],
            "meta": {"count": 1}
        })
        .to_string();

        let output = redact_for_gate(&tool_result, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(parsed, serde_json::json!({"meta": {"count": 1}}));
        assert!(!output.contains("Bob"), "{output}");
    }

    #[test]
    fn test_tester_relaxed_egress_keeps_rows_for_generate_natural_language() {
        let gate = relaxed_gate();
        let nl2sql_context = serde_json::json!({
            "password": "nl-secret",
            "resultRows": [{"city": "Shanghai"}],
        })
        .to_string();

        let output = redact_for_gate(&nl2sql_context, &gate);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(
            parsed,
            serde_json::json!({"resultRows": [{"city": "Shanghai"}]})
        );
        assert!(!output.contains("nl-secret"), "{output}");
    }

    // ── strict ↔ relaxed toggle ──

    #[test]
    fn test_tester_egress_toggle_strict_to_relaxed_preserves_rows_immediately() {
        let payload = serde_json::json!({
            "rows": [{"id": 99}],
            "token": "toggle-secret",
        })
        .to_string();

        let strict = redact_for_gate(&payload, &strict_gate());
        let relaxed = redact_for_gate(&payload, &relaxed_gate());

        let strict_parsed: Value = serde_json::from_str(&strict).expect("strict JSON");
        let relaxed_parsed: Value = serde_json::from_str(&relaxed).expect("relaxed JSON");

        assert_eq!(strict_parsed, serde_json::json!({}));
        assert_eq!(relaxed_parsed, serde_json::json!({"rows": [{"id": 99}]}));
        assert!(!strict.contains("toggle-secret"), "{strict}");
        assert!(!relaxed.contains("toggle-secret"), "{relaxed}");
        assert_ne!(strict, relaxed);
    }

    #[test]
    fn test_tester_egress_toggle_relaxed_to_strict_strips_rows_immediately() {
        let payload = serde_json::json!({
            "queryResultData": [{"score": 10}],
            "safeNote": "reference only",
        })
        .to_string();

        let relaxed = redact_for_gate(&payload, &relaxed_gate());
        let strict = redact_for_gate(&payload, &strict_gate());

        let relaxed_parsed: Value = serde_json::from_str(&relaxed).expect("relaxed JSON");
        let strict_parsed: Value = serde_json::from_str(&strict).expect("strict JSON");

        assert_eq!(
            relaxed_parsed,
            serde_json::json!({
                "queryResultData": [{"score": 10}],
                "safeNote": "reference only",
            })
        );
        assert_eq!(
            strict_parsed,
            serde_json::json!({"safeNote": "reference only"})
        );
        assert_ne!(relaxed, strict);
    }

    // ── sample-masked gate ──

    #[test]
    fn test_safety_gate_sample_masked_limits_rows() {
        let gate = sample_gate(2);
        let payload = serde_json::json!({
            "rows": [
                {"id": 1, "name": "user1"},
                {"id": 2, "name": "user2"},
                {"id": 3, "name": "user3"},
                {"id": 4, "name": "user4"},
                {"id": 5, "name": "user5"},
            ],
            "tableName": "users",
        })
        .to_string();

        let result = redact_for_gate(&payload, &gate);
        let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
        assert_eq!(parsed["rows"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["tableName"], "users");
    }

    #[test]
    fn test_safety_gate_unrestricted_allows_rows_but_strips_credentials() {
        let gate = relaxed_gate();
        let payload = serde_json::json!({
            "rows": [{"id": 1, "password": "super-secret"}],
            "token": "secret-token",
            "tableName": "users",
        })
        .to_string();

        let result = redact_for_gate(&payload, &gate);
        let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
        assert_eq!(parsed["rows"].as_array().unwrap().len(), 1);
        assert!(parsed["token"].is_null());
        assert_eq!(parsed["tableName"], "users");
    }

    // ── max_context_bytes configurable ──

    #[test]
    fn custom_max_context_bytes_limits_output() {
        let gate = AiSafetyGateConfig {
            max_context_bytes: 50,
            ..Default::default()
        };
        let input = "x".repeat(100);
        let output = redact_for_gate(&input, &gate);
        assert_eq!(output.len(), 50);
    }
}
