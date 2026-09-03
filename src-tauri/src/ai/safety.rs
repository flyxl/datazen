//! Safety boundary for caller-provided text used by AI prompts.
//!
//! AI command handlers must not trust a UI-side redaction step.  Keep this
//! module independent from Tauri and the provider implementations so every
//! command can sanitize text immediately before it becomes prompt content.

use regex::Regex;
use serde_json::{Map, Value};
use std::sync::OnceLock;

const MAX_AI_TEXT_BYTES: usize = 4_000;
const MAX_JSON_DEPTH: usize = 4;
const MAX_JSON_ARRAY_ITEMS: usize = 100;
const MAX_JSON_OBJECT_KEYS: usize = 100;

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

const RESULT_KEYS: &[&str] = &[
    "data",
    "row",
    "rows",
    "record",
    "records",
    "result",
    "results",
    "resultset",
    "resultsets",
    "resultsset",
    "resultssets",
    "resultrow",
    "resultrows",
    "resultsrow",
    "resultsrows",
    "resultdata",
    "resultsdata",
    "queryresult",
    "queryresults",
    "queryresultset",
    "queryresultsets",
    "queryresultsset",
    "queryresultssets",
    "queryresultrow",
    "queryresultrows",
    "queryresultsrow",
    "queryresultsrows",
    "queryresultdata",
    "queryresultsdata",
    "samplerow",
    "samplerows",
    "rawoutput",
    "executionoutput",
    "payload",
];

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

fn is_sensitive_key(key: &str) -> bool {
    let words = key_words(key);
    words
        .iter()
        .any(|word| SECRET_WORDS.iter().any(|secret| word == secret))
        || matches!(
            words.join("").as_str(),
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

fn is_result_key(key: &str) -> bool {
    let canonical = key_words(key).join("");
    RESULT_KEYS.iter().any(|result| *result == canonical)
}

fn consume_assigned_value(value: &str, start: usize) -> usize {
    let mut index = start;
    while index < value.len() {
        let ch = value[index..].chars().next().expect("valid char boundary");
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
            let ch = value[cursor..].chars().next().expect("valid char boundary");
            if ch == '\\' {
                cursor += ch.len_utf8();
                if cursor < value.len() {
                    cursor += value[cursor..]
                        .chars()
                        .next()
                        .expect("valid char boundary")
                        .len_utf8();
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
        let ch = value[cursor..].chars().next().expect("valid char boundary");
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

fn sanitize_json(value: Value, depth: usize, strict_egress: bool) -> Value {
    if depth >= MAX_JSON_DEPTH && value.is_object() {
        return Value::String("[truncated]".into());
    }

    match value {
        Value::String(value) => Value::String(redact_plain_text(&value)),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .take(MAX_JSON_ARRAY_ITEMS)
                .map(|value| sanitize_json(value, depth + 1, strict_egress))
                .collect(),
        ),
        Value::Object(values) => {
            let mut sanitized = Map::new();
            for (key, value) in values.into_iter().take(MAX_JSON_OBJECT_KEYS) {
                if is_sensitive_key(&key) || (strict_egress && is_result_key(&key)) {
                    continue;
                }
                sanitized.insert(key, sanitize_json(value, depth + 1, strict_egress));
            }
            Value::Object(sanitized)
        }
        other => other,
    }
}

fn redact_json_text(value: &str, strict_egress: bool) -> Option<String> {
    let trimmed = value.trim();
    if !((trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('"') && trimmed.ends_with('"')))
    {
        return None;
    }

    serde_json::from_str::<Value>(trimmed)
        .ok()
        .and_then(|parsed| serde_json::to_string(&sanitize_json(parsed, 0, strict_egress)).ok())
}

fn truncate_to_bytes(value: String) -> String {
    if value.len() <= MAX_AI_TEXT_BYTES {
        return value;
    }
    let mut end = MAX_AI_TEXT_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

/// Return bounded prompt text with credentials removed and, by default, result rows stripped.
pub(crate) fn redact_for_ai(value: &str) -> String {
    redact_for_egress(value, true)
}

/// Sanitize caller-provided text before it becomes AI prompt content.
///
/// Credentials are always redacted. When `strict_egress` is true (the default), structured
/// query result / payload fields are removed as well.
pub(crate) fn redact_for_egress(value: &str, strict_egress: bool) -> String {
    truncate_to_bytes(
        redact_json_text(value, strict_egress).unwrap_or_else(|| redact_plain_text(value)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sensitive_key_assignments() {
        for (input, secret) in [
            ("password=db-secret", "db-secret"),
            ("apiKey=sk-live-123", "sk-live-123"),
            ("Authorization: Bearer bearer-secret", "bearer-secret"),
        ] {
            let output = redact_for_ai(input);
            assert!(!output.contains(secret), "{output}");
            assert!(output.contains("[REDACTED]"), "{output}");
        }

        let json_output = redact_for_ai("{\"clientSecret\":\"top-secret\"}");
        assert!(!json_output.contains("top-secret"), "{json_output}");
        assert_eq!(json_output, "{}");
    }

    #[test]
    fn redacts_uri_credentials_and_query_secrets() {
        let input =
            "url=mysql://user:uri-secret@db.example/app?token=query-secret&api_key=key-secret";
        let output = redact_for_ai(input);
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
        let output = redact_for_ai("Authorization: bearer abc.def.ghi");
        assert!(!output.contains("abc.def.ghi"), "{output}");
        assert!(output.contains("[REDACTED]"), "{output}");

        let standalone = redact_for_ai("prefix bearer abc.def.ghi");
        assert!(!standalone.contains("abc.def.ghi"), "{standalone}");
        assert!(standalone.contains("bearer [REDACTED]"), "{standalone}");
    }

    #[test]
    fn redacts_sensitive_assignments_and_connection_credentials() {
        let input = "query failed: apiToken=prefix-token; SELECT * FROM t WHERE password = 'db-secret' AND apiToken=token-secret; \
            url=mysql://user:uri-secret@db.example/app?token=query-secret; Bearer bearer-secret";
        let output = redact_for_ai(input);

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

    #[test]
    fn removes_nested_json_secrets_and_result_rows_in_strict_mode() {
        let input = serde_json::json!({
            "apiToken": "API_TOKEN_HEAD\"TAIL\\END",
            "nested": {"password": "PASSWORD_SECRET", "safe": "keep"},
            "resultRows": [{"email": "user@example.test"}],
        })
        .to_string();
        let output = redact_for_ai(&input);
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
        let output = redact_for_egress(&input, false);
        let parsed: Value = serde_json::from_str(&output).expect("safe JSON");

        assert_eq!(
            parsed,
            serde_json::json!({"rows": [{"email": "user@example.test"}]})
        );
        assert!(!output.contains("PASSWORD_SECRET"), "{output}");
    }

    #[test]
    fn bounds_large_prompt_text_without_panicking_on_utf8() {
        let output = redact_for_ai(&"表".repeat(3_000));
        assert!(output.len() <= MAX_AI_TEXT_BYTES);
        assert!(output.is_char_boundary(output.len()));
    }

    // ── tester: sanitization boundaries ──

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

        let output = redact_for_ai(&input);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(
            parsed,
            serde_json::json!({"level1": {"level2": {"level3": {"safe": "keep-me"}}}})
        );
        for secret in ["root-secret", "level1-secret", "level2-secret", "level3-secret"] {
            assert!(!output.contains(secret), "{output}");
        }
    }

    #[test]
    fn test_tester_deep_json_truncates_objects_at_max_depth() {
        let input = serde_json::json!({
            "a": {"b": {"c": {"d": {"e": "too-deep", "safe": "visible"}}}}
        })
        .to_string();

        let output = redact_for_egress(&input, false);
        let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(
            parsed,
            serde_json::json!({"a": {"b": {"c": {"d": "[truncated]"}}}})
        );
        assert!(!output.contains("too-deep"), "{output}");
    }

    #[test]
    fn test_tester_oversized_ascii_payload_truncates_at_byte_boundary() {
        let input = "x".repeat(MAX_AI_TEXT_BYTES + 500);
        let output = redact_for_egress(&input, true);

        assert_eq!(output.len(), MAX_AI_TEXT_BYTES);
        assert!(output.is_char_boundary(output.len()));
        assert!(input.starts_with(&output));
    }

    #[test]
    fn test_tester_oversized_multibyte_payload_respects_char_boundaries() {
        // 3-byte CJK chars: ensure truncation never splits a codepoint.
        let unit = "表";
        let repeat = MAX_AI_TEXT_BYTES / unit.len() + 10;
        let input = unit.repeat(repeat);
        let output = redact_for_egress(&input, true);

        assert!(output.len() <= MAX_AI_TEXT_BYTES);
        assert!(output.is_char_boundary(output.len()));
        assert!(input.starts_with(&output));
        assert!(std::str::from_utf8(output.as_bytes()).is_ok());
    }

    #[test]
    fn test_tester_invalid_json_fallback_still_redacts_plain_secrets() {
        let input = r#"{"broken": true, password=leaked-secret"#;
        let output = redact_for_egress(input, true);

        assert!(!output.contains("leaked-secret"), "{output}");
        assert!(output.contains("[REDACTED]"), "{output}");
    }

    // ── tester: egress policy (chat / generate path simulation) ──

    #[test]
    fn test_tester_strict_egress_strips_result_rows_like_chat_user_message() {
        let chat_payload = serde_json::json!({
            "question": "summarize",
            "queryResults": [{"id": 1, "email": "alice@corp.test"}],
            "password": "chat-secret"
        })
        .to_string();

        let strict = redact_for_egress(&chat_payload, true);
        let parsed: Value = serde_json::from_str(&strict).expect("valid JSON");

        assert_eq!(parsed, serde_json::json!({"question": "summarize"}));
        assert!(!strict.contains("alice@corp.test"), "{strict}");
        assert!(!strict.contains("chat-secret"), "{strict}");
    }

    #[test]
    fn test_tester_strict_egress_strips_tool_result_payload_like_chat_loop() {
        let tool_result = serde_json::json!({
            "rows": [{"name": "Bob"}],
            "meta": {"count": 1}
        })
        .to_string();

        let strict = redact_for_egress(&tool_result, true);
        let parsed: Value = serde_json::from_str(&strict).expect("valid JSON");

        assert_eq!(parsed, serde_json::json!({"meta": {"count": 1}}));
        assert!(!strict.contains("Bob"), "{strict}");
    }

    #[test]
    fn test_tester_relaxed_egress_keeps_rows_for_generate_natural_language() {
        let nl2sql_context = serde_json::json!({
            "password": "nl-secret",
            "resultRows": [{"city": "Shanghai"}],
        })
        .to_string();

        let relaxed = redact_for_egress(&nl2sql_context, false);
        let parsed: Value = serde_json::from_str(&relaxed).expect("valid JSON");

        assert_eq!(
            parsed,
            serde_json::json!({"resultRows": [{"city": "Shanghai"}]})
        );
        assert!(!relaxed.contains("nl-secret"), "{relaxed}");
    }

    #[test]
    fn test_tester_redact_for_ai_matches_strict_egress_default() {
        let sample = serde_json::json!({
            "api_key": "secret-value",
            "data": [{"value": 42}],
        })
        .to_string();

        assert_eq!(redact_for_ai(&sample), redact_for_egress(&sample, true));
    }

    // ── tester: strict ↔ relaxed toggle immediate effect ──

    #[test]
    fn test_tester_egress_toggle_strict_to_relaxed_preserves_rows_immediately() {
        let payload = serde_json::json!({
            "rows": [{"id": 99}],
            "token": "toggle-secret",
        })
        .to_string();

        let strict = redact_for_egress(&payload, true);
        let relaxed = redact_for_egress(&payload, false);

        let strict_parsed: Value = serde_json::from_str(&strict).expect("strict JSON");
        let relaxed_parsed: Value = serde_json::from_str(&relaxed).expect("relaxed JSON");

        assert_eq!(strict_parsed, serde_json::json!({}));
        assert_eq!(
            relaxed_parsed,
            serde_json::json!({"rows": [{"id": 99}]})
        );
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

        let relaxed = redact_for_egress(&payload, false);
        let strict = redact_for_egress(&payload, true);

        let relaxed_parsed: Value = serde_json::from_str(&relaxed).expect("relaxed JSON");
        let strict_parsed: Value = serde_json::from_str(&strict).expect("strict JSON");

        assert_eq!(
            relaxed_parsed,
            serde_json::json!({
                "queryResultData": [{"score": 10}],
                "safeNote": "reference only",
            })
        );
        assert_eq!(strict_parsed, serde_json::json!({"safeNote": "reference only"}));
        assert_ne!(relaxed, strict);
    }
}
