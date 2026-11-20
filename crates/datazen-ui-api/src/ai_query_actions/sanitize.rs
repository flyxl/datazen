//! Secret/result-row redaction — the React file's `sanitize*`/`redact*` half.
//!
//! This module never touches SQL text. The editor SQL is preserved
//! byte-for-byte by `context::build_query_diagnosis_context` and only its
//! *redacted copy* is produced here; Fix SQL / Retry consume the original.
//!
//! React uses several regexes. They are hand-transcribed here with small
//! byte scanners so the crate keeps no `regex` dependency (and stays wasm
//! safe). The scanners follow the same char classes and greedy behaviour:
//!
//! - `keyWords`: camelCase split on `[a-z0-9][A-Z]`, then split on
//!   non-alphanumerics, lowercase each token;
//! - `SENSITIVE_ASSIGNMENT_PATTERN`: `(?<!\w)(?:\\["'])?([A-Za-z][\w.-]*)(?:\\["'])?\s*(?:\\?:|=)\s*`;
//! - URI credentials `scheme://user:pass@` and `Bearer <token>`;
//! - `sanitizeHost`'s anchored `(^|//)user:pass@`.
//!
//! `serde_json::Value` is an owned acyclic tree, so React's `seen` cycle set
//! has no counterpart: a cycle cannot be constructed in the port's input.

use serde_json::{Map, Value};

use super::types::SanitizedJson;

const MAX_SAFE_TEXT_LENGTH: usize = 4_000;
const MAX_SCHEMA_DEPTH: usize = 4;
const MAX_SCHEMA_ARRAY_ITEMS: usize = 100;
const MAX_SCHEMA_OBJECT_KEYS: usize = 100;

/// React `SECRET_KEY_WORDS`: any single token equal to one of these makes the
/// whole key sensitive. Note `key` is included — `myKeyValue` is sensitive.
const SECRET_KEY_WORDS: [&str; 11] = [
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

/// React `SECRET_KEY_PATTERN` suffixes, in source order.
const SECRET_SUFFIXES: [&str; 11] = [
    "token",
    "key",
    "secret",
    "password",
    "passwd",
    "pwd",
    "credential",
    "credentials",
    "authorization",
    "bearer",
    "passphrase",
];

/// React's optional prefixes (the empty string is the "no prefix" arm).
const SECRET_PREFIXES: [&str; 10] = [
    "",
    "api",
    "auth",
    "oauth",
    "access",
    "refresh",
    "client",
    "private",
    "encryption",
    "signing",
];

/// React `RESULT_KEY_NAMES`: object keys whose *values* are result rows and
/// must never reach an AI prompt. All names are already lowercase singles so
/// `keyWords(key).join('')` is the name itself for the canonical spellings.
const RESULT_KEY_NAMES: [&str; 34] = [
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

/// React `keyWords`: `[a-z0-9][A-Z]` → insert a space, split on
/// `[^a-zA-Z0-9]+`, drop empties, lowercase.
fn key_words(key: &str) -> Vec<String> {
    let chars: Vec<char> = key.chars().collect();
    let mut spaced = String::with_capacity(key.len() + 2);
    for (index, character) in chars.iter().enumerate() {
        if index > 0
            && character.is_ascii_uppercase()
            && (chars[index - 1].is_ascii_lowercase() || chars[index - 1].is_ascii_digit())
        {
            spaced.push(' ');
        }
        spaced.push(*character);
    }
    spaced
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// React `isSensitiveKey`.
fn is_sensitive_key(key: &str) -> bool {
    let words = key_words(key);
    if words
        .iter()
        .any(|word| SECRET_KEY_WORDS.contains(&word.as_str()))
    {
        return true;
    }
    let joined = words.concat();
    for suffix in SECRET_SUFFIXES {
        if let Some(prefix) = joined.strip_suffix(suffix) {
            if SECRET_PREFIXES.contains(&prefix) {
                return true;
            }
        }
    }
    false
}

/// React `isResultKey`.
fn is_result_key(key: &str) -> bool {
    let joined = key_words(key).concat();
    RESULT_KEY_NAMES.contains(&joined.as_str())
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_js_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0c | 0x0b)
}

fn skip_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && is_js_whitespace(bytes[index]) {
        index += 1;
    }
    index
}

/// Consume an optional (possibly backslash-escaped) quote at `index`.
/// Returns `(new_index, quote_byte)` where `quote_byte` is `None` if no
/// quote was present.
fn consume_optional_quote(bytes: &[u8], index: usize) -> (usize, Option<u8>) {
    if bytes.get(index) == Some(&b'\\') && matches!(bytes.get(index + 1), Some(b'"') | Some(b'\''))
    {
        (index + 2, bytes.get(index + 1).copied())
    } else if matches!(bytes.get(index), Some(b'"') | Some(b'\'')) {
        (index + 1, bytes.get(index).copied())
    } else {
        (index, None)
    }
}

/// Match `SENSITIVE_ASSIGNMENT_PATTERN` at `start`. Returns the key slice and
/// the byte index where the assigned value begins.
fn match_assignment<'a>(value: &'a str, start: usize) -> Option<(&'a str, usize)> {
    let bytes = value.as_bytes();
    // Lookbehind (?<!\w).
    if start > 0 && is_word_byte(bytes[start - 1]) {
        return None;
    }
    let (after_open, _) = consume_optional_quote(bytes, start);
    let key_start = after_open;
    if !bytes.get(key_start).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let mut index = key_start + 1;
    while index < bytes.len()
        && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'.' | b'-'))
    {
        index += 1;
    }
    let key_end = index;
    let (after_close, _) = consume_optional_quote(bytes, index);
    index = skip_whitespace(bytes, after_close);
    match bytes.get(index) {
        Some(b'=') => index += 1,
        Some(b':') => index += 1,
        Some(b'\\') if bytes.get(index + 1) == Some(&b':') => index += 2,
        _ => return None,
    }
    index = skip_whitespace(bytes, index);
    Some((&value[key_start..key_end], index))
}

/// React `consumeAssignedValue`: skip whitespace, consume an optional quoted
/// string (handling backslash escapes) or stop at a delimiter.
fn consume_assigned_value(value: &str, start: usize) -> usize {
    let bytes = value.as_bytes();
    let mut index = skip_whitespace(bytes, start);
    let (after_quote, quote) = if bytes.get(index) == Some(&b'\\')
        && matches!(bytes.get(index + 1), Some(b'"') | Some(b'\''))
    {
        (index + 2, bytes.get(index + 1).copied())
    } else if matches!(bytes.get(index), Some(b'"') | Some(b'\'') | Some(b'`')) {
        (index + 1, bytes.get(index).copied())
    } else {
        (index, None)
    };
    if let Some(quote) = quote {
        index = after_quote;
        while index < bytes.len() {
            if bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
            } else if bytes[index] == quote {
                return index + 1;
            } else {
                index += 1;
            }
        }
        return index;
    }
    while index < bytes.len()
        && !is_js_whitespace(bytes[index])
        && !matches!(bytes[index], b',' | b';' | b')' | b']' | b'}')
    {
        index += 1;
    }
    index
}

/// React `redactSensitiveAssignments`: replace the value of a sensitive
/// assignment with `[REDACTED]`, leaving all other text untouched.
fn redact_sensitive_assignments(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        if let Some((key, value_start)) = match_assignment(value, index) {
            if is_sensitive_key(key) && index >= cursor {
                let value_end = consume_assigned_value(value, value_start);
                result.push_str(&value[cursor..index]);
                result.push_str("[REDACTED]");
                cursor = value_end;
                index = value_end;
                continue;
            }
        }
        index += 1;
    }
    result.push_str(&value[cursor..]);
    result
}

fn is_uri_user_stop(byte: u8) -> bool {
    byte == b'/' || byte == b':' || byte == b'@' || is_js_whitespace(byte)
}

fn is_scheme_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-' | b'_')
}

/// React's `([a-z][\w+.-]*:\/\/)(?:[^/\s:@]+)(?::[^@\s/]*)?@` replacement.
fn redact_uri_credentials(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0usize;
    let mut index = 0usize;
    while index + 2 < bytes.len() {
        if bytes[index] == b':' && bytes[index + 1] == b'/' && bytes[index + 2] == b'/' {
            let mut scheme_start = index;
            while scheme_start > 0 && is_scheme_byte(bytes[scheme_start - 1]) {
                scheme_start -= 1;
            }
            if scheme_start < index && bytes[scheme_start].is_ascii_alphabetic() {
                let user_start = index + 3;
                let mut after_user = user_start;
                while after_user < bytes.len() && !is_uri_user_stop(bytes[after_user]) {
                    after_user += 1;
                }
                if after_user > user_start {
                    let mut end = after_user;
                    if bytes.get(end) == Some(&b':') {
                        end += 1;
                        while end < bytes.len()
                            && bytes[end] != b'@'
                            && bytes[end] != b'/'
                            && !is_js_whitespace(bytes[end])
                        {
                            end += 1;
                        }
                    }
                    if bytes.get(end) == Some(&b'@') {
                        result.push_str(&value[cursor..user_start]);
                        result.push_str("[REDACTED]@");
                        cursor = end + 1;
                        index = end + 1;
                        continue;
                    }
                }
            }
        }
        index += 1;
    }
    result.push_str(&value[cursor..]);
    result
}

/// React's `(?:\bBearer\s+)[^\s,;)]+` replacement (case-insensitive).
fn redact_bearer(value: &str) -> String {
    let lowered = value.to_ascii_lowercase();
    let bytes = lowered.as_bytes();
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0usize;
    let mut index = 0usize;
    while index + 6 <= bytes.len() {
        if &bytes[index..index + 6] == b"bearer" {
            let boundary_ok = index == 0 || !is_word_byte(bytes[index - 1]);
            let mut after_word = index + 6;
            let ws_start = after_word;
            after_word = skip_whitespace(bytes, after_word);
            if boundary_ok && after_word > ws_start {
                let token_start = after_word;
                while after_word < bytes.len()
                    && !is_js_whitespace(bytes[after_word])
                    && !matches!(bytes[after_word], b',' | b';' | b')')
                {
                    after_word += 1;
                }
                if after_word > token_start {
                    result.push_str(&value[cursor..index]);
                    result.push_str("Bearer [REDACTED]");
                    cursor = after_word;
                    index = after_word;
                    continue;
                }
            }
        }
        index += 1;
    }
    result.push_str(&value[cursor..]);
    result
}

/// React `redactSensitivePlainText`.
fn redact_sensitive_plain_text(value: &str) -> String {
    redact_sensitive_assignments(&redact_bearer(&redact_uri_credentials(value)))
}

/// `JSON.stringify`-compatible string literal (no `serde` Result to unwrap).
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// JS `String(number)` for the finite numbers that can reach a fingerprint.
fn js_number(number: &serde_json::Number) -> String {
    if let Some(integer) = number.as_i64() {
        return integer.to_string();
    }
    if let Some(unsigned) = number.as_u64() {
        return unsigned.to_string();
    }
    match number.as_f64() {
        Some(float) if float.is_finite() => {
            if float == 0.0 {
                "0".to_owned()
            } else if float.fract() == 0.0 && float.abs() < 1e21 {
                format!("{}", float as i128)
            } else {
                format!("{}", float)
            }
        }
        _ => "null".to_owned(),
    }
}

/// React `canonicalize`: sorted object keys, arrays in order, JSON string
/// literals, `String(number)` for numbers, `null` for everything else.
pub(crate) fn canonicalize(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => {
            if *flag {
                "true".to_owned()
            } else {
                "false".to_owned()
            }
        }
        Value::Number(number) => js_number(number),
        Value::String(text) => json_string(text),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonicalize).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Object(object) => {
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|key| format!("{}:{}", json_string(key), canonicalize(&object[key])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

/// React's `.slice(0, 4000)` counts UTF-16 code units, not scalar values.
fn truncate_utf16(mut value: String, limit: usize) -> String {
    let mut units = 0usize;
    let mut end = 0usize;
    for (index, character) in value.char_indices() {
        let width = character.len_utf16();
        if units + width > limit {
            break;
        }
        units += width;
        end = index + character.len_utf8();
    }
    value.truncate(end);
    value
}

/// React `sanitizeSerializedJsonValue`: scalar passthrough (strings get the
/// *plain-text* redactor), bounded arrays/objects, result/secret keys dropped.
fn sanitize_serialized_json_value(value: &Value, depth: usize) -> SanitizedJson {
    if depth >= MAX_SCHEMA_DEPTH {
        return Value::String("[truncated]".to_owned());
    }
    match value {
        Value::Null => Value::Null,
        Value::String(text) => Value::String(redact_sensitive_plain_text(text)),
        Value::Bool(flag) => Value::Bool(*flag),
        Value::Number(number) => number.as_f64().map_or(Value::Null, |_| value.clone()),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(MAX_SCHEMA_ARRAY_ITEMS)
                .map(|item| sanitize_serialized_json_value(item, depth + 1))
                .collect(),
        ),
        Value::Object(object) => {
            let mut result = Map::new();
            for (key, child) in object.iter().take(MAX_SCHEMA_OBJECT_KEYS) {
                if is_sensitive_key(key) || is_result_key(key) {
                    continue;
                }
                result.insert(
                    key.clone(),
                    sanitize_serialized_json_value(child, depth + 1),
                );
            }
            Value::Object(result)
        }
    }
}

/// React `redactSerializedJson`: only a fully JSON-looking string is parsed;
/// a parse failure is `null` (the caller then uses the plain-text redactor).
fn redact_serialized_json(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let looks_json = (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2);
    if !looks_json {
        return None;
    }
    let parsed: Value = serde_json::from_str(trimmed).ok()?;
    let sanitized = sanitize_serialized_json_value(&parsed, 0);
    serde_json::to_string(&sanitized).ok()
}

/// React `redactSensitiveText`: the JSON-aware redactor when the string is
/// itself a JSON document, the plain-text redactor otherwise, bounded to
/// 4,000 UTF-16 units.
pub(crate) fn redact_sensitive_text(value: &str) -> String {
    let redacted =
        redact_serialized_json(value).unwrap_or_else(|| redact_sensitive_plain_text(value));
    truncate_utf16(redacted, MAX_SAFE_TEXT_LENGTH)
}

/// React `sanitizeSchemaValue`: like the serialized walker, but strings go
/// through the JSON-aware redactor and null children of non-null parents are
/// dropped. Depth-limited to four levels.
fn sanitize_schema_value(value: &Value, depth: usize) -> SanitizedJson {
    match value {
        Value::Null => Value::Null,
        Value::String(text) => Value::String(redact_sensitive_text(text)),
        Value::Bool(flag) => Value::Bool(*flag),
        Value::Number(number) => number.as_f64().map_or(Value::Null, |_| value.clone()),
        Value::Array(_) | Value::Object(_) if depth >= MAX_SCHEMA_DEPTH => {
            Value::String("[truncated]".to_owned())
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(MAX_SCHEMA_ARRAY_ITEMS)
                .map(|item| sanitize_schema_value(item, depth + 1))
                .collect(),
        ),
        Value::Object(object) => {
            let mut result = Map::new();
            for (key, child) in object.iter().take(MAX_SCHEMA_OBJECT_KEYS) {
                if is_sensitive_key(key) || is_result_key(key) {
                    continue;
                }
                let sanitized = sanitize_schema_value(child, depth + 1);
                if !sanitized.is_null() || child.is_null() {
                    result.insert(key.clone(), sanitized);
                }
            }
            Value::Object(result)
        }
    }
}

/// React `sanitizeSchemaContext`.
pub(crate) fn sanitize_schema_context(value: Option<&Value>) -> SanitizedJson {
    value.map_or(Value::Null, |value| sanitize_schema_value(value, 0))
}

/// React `sanitizeHost`: strip anchored `user:pass@` and cap at 512 units.
pub(crate) fn sanitize_host(value: Option<&str>) -> Option<String> {
    let value = value?;
    if value.is_empty() {
        return None;
    }
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        // React's `(^|\/\/)` prefers the `//` arm even at position 0.
        let anchor = if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'/') {
            index + 2
        } else if index == 0 {
            0usize
        } else {
            index += 1;
            continue;
        };
        let mut user_end = anchor;
        while user_end < bytes.len()
            && bytes[user_end] != b'/'
            && !is_js_whitespace(bytes[user_end])
            && bytes[user_end] != b':'
            && bytes[user_end] != b'@'
        {
            user_end += 1;
        }
        if user_end > anchor && bytes.get(user_end) == Some(&b':') {
            let mut pass_end = user_end + 1;
            while pass_end < bytes.len()
                && bytes[pass_end] != b'@'
                && bytes[pass_end] != b'/'
                && !is_js_whitespace(bytes[pass_end])
            {
                pass_end += 1;
            }
            if bytes.get(pass_end) == Some(&b'@') {
                result.push_str(&value[cursor..anchor]);
                result.push_str("[REDACTED]@");
                cursor = pass_end + 1;
                index = pass_end + 1;
                continue;
            }
        }
        index += 1;
    }
    result.push_str(&value[cursor..]);
    Some(truncate_utf16(result, 512))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sensitive_keys_follow_the_word_and_pattern_vocabulary() {
        for key in [
            "password",
            "Password",
            "apiKey",
            "auth-token",
            "client_secret",
            "ACCESS_TOKEN",
            "encryptionKey",
            "dbPassword",
            "passphrase",
            "oauth_token",
        ] {
            assert!(is_sensitive_key(key), "{}", key);
        }
        for key in ["name", "host", "port", "serverVersion", "sql", "database"] {
            assert!(!is_sensitive_key(key), "{}", key);
        }
        assert!(is_result_key("rows"));
        assert!(is_result_key("resultSets"));
        assert!(is_result_key("queryResultRows"));
        assert!(!is_result_key("tableName"));
    }

    #[test]
    fn every_result_name_and_secret_prefix_suffix_pair_is_pinned() {
        for key in RESULT_KEY_NAMES {
            assert!(is_result_key(key), "{}", key);
        }
        for prefix in [
            "api",
            "auth",
            "oauth",
            "access",
            "refresh",
            "client",
            "private",
            "encryption",
            "signing",
        ] {
            for suffix in SECRET_SUFFIXES {
                let key = format!("{prefix}{suffix}");
                assert!(is_sensitive_key(&key), "{}", key);
            }
        }
        // camelCase spellings take the `keyWords` split before the pattern.
        assert!(is_sensitive_key("apiToken"));
        assert!(is_sensitive_key("signingKey"));
        assert!(is_sensitive_key("refreshCredential"));
    }

    #[test]
    fn assignments_uri_and_bearer_secrets_are_redacted() {
        // React replaces the whole matched assignment (key + separator +
        // value) with `[REDACTED]`, so the key itself disappears too.
        assert_eq!(
            redact_sensitive_plain_text("password= hunter2 ; host=db"),
            "[REDACTED] ; host=db"
        );
        assert_eq!(
            redact_sensitive_plain_text("{\"api_key\": \"abc123\"}"),
            "{[REDACTED]}"
        );
        assert_eq!(
            redact_sensitive_plain_text("postgres://alice:s3cr3t@db.local/x"),
            "postgres://[REDACTED]@db.local/x"
        );
        // The Bearer pass runs first; then `Authorization:` is itself a
        // sensitive assignment, so its value (the first Bearer token word)
        // is redacted again.
        assert_eq!(
            redact_sensitive_plain_text("Authorization: Bearer abc.def"),
            "[REDACTED] [REDACTED]"
        );
        // A non-sensitive assignment keeps its value verbatim.
        assert_eq!(
            redact_sensitive_plain_text("host=db.local"),
            "host=db.local"
        );
    }

    #[test]
    fn schema_context_drops_result_rows_and_secret_fields() {
        let input = json!({
            "columns": [{ "name": "id", "type": "int" }],
            "rows": [{ "id": 1 }],
            "apiKey": "abc",
            "nested": { "password": "x", "table": "users" },
            "note": "password=secret",
        });
        let sanitized = sanitize_schema_context(Some(&input));
        let object = sanitized
            .as_object()
            .expect("sanitized schema is an object");
        assert!(!object.contains_key("rows"));
        assert!(!object.contains_key("apiKey"));
        assert_eq!(object["nested"]["table"], "users");
        assert!(!object["nested"]
            .as_object()
            .unwrap()
            .contains_key("password"));
        assert_eq!(object["note"], "[REDACTED]");
        assert_eq!(object["columns"][0]["name"], "id");
    }

    #[test]
    fn json_shaped_text_uses_the_structured_redactor() {
        let text = r#"{"token":"abc123","host":"db.local"}"#;
        let redacted = redact_sensitive_text(text);
        let parsed: Value = serde_json::from_str(&redacted).unwrap();
        assert_eq!(parsed["host"], "db.local");
        assert!(parsed.get("token").is_none());
        // Non-JSON text falls through to the plain-text redactor (which
        // drops the matched key/separator along with the value).
        assert_eq!(redact_sensitive_text("token=abc"), "[REDACTED]");
    }

    #[test]
    fn depth_array_and_key_bounds_hold() {
        let deep = json!({"a":{"b":{"c":{"d":{"e":"x"}}}}});
        let sanitized = sanitize_schema_context(Some(&deep));
        assert_eq!(sanitized["a"]["b"]["c"]["d"], "[truncated]");
        let many = json!({"x": (0..150).collect::<Vec<_>>()});
        assert_eq!(
            sanitize_schema_context(Some(&many))["x"]
                .as_array()
                .map(Vec::len),
            Some(100)
        );
        let mut object = Map::new();
        for index in 0..150 {
            object.insert(format!("k{index:03}"), json!(index));
        }
        assert_eq!(
            sanitize_schema_context(Some(&Value::Object(object)))
                .as_object()
                .map(Map::len),
            Some(100)
        );
        // UTF-16 truncation matches JS `.slice(0, 4000)`: a surrogate pair
        // counts as two units, not one scalar value.
        let long = "x".repeat(4_100);
        assert_eq!(redact_sensitive_text(&long).chars().count(), 4_000);
        assert_eq!(
            redact_sensitive_text(&"😀".repeat(2_001)).chars().count(),
            2_000
        );
    }

    #[test]
    fn host_credentials_are_stripped_without_damaging_plain_hosts() {
        assert_eq!(
            sanitize_host(Some("alice:pw@db.local")),
            Some("[REDACTED]@db.local".to_owned())
        );
        assert_eq!(
            sanitize_host(Some("//alice:pw@db.local")),
            Some("//[REDACTED]@db.local".to_owned())
        );
        assert_eq!(sanitize_host(Some("db.local")), Some("db.local".to_owned()));
        assert_eq!(sanitize_host(None), None);
    }

    #[test]
    fn canonicalize_sorts_keys_and_matches_js_number_strings() {
        assert_eq!(
            canonicalize(&json!({"b": 1, "a": "x"})),
            "{\"a\":\"x\",\"b\":1}"
        );
        assert_eq!(canonicalize(&json!([1, true, null])), "[1,true,null]");
        assert_eq!(canonicalize(&json!(1.5)), "1.5");
    }
}
