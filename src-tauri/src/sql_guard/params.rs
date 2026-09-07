//! Parameter binding: v2 occurrence-based binder with legacy payload compatibility.

use crate::sql_guard::scanner::{is_in_non_replaceable, non_replaceable_ranges, skip_quoted};
use serde_json::Value as JsonValue;

/// Substitute bind placeholders with JSON param values as SQL literals.
///
/// Accepts v2 payload `{ version: 2, values, occurrences }`, legacy object map,
/// or legacy array for positional `?` / `$n` substitution.
pub fn apply_params(sql: &str, params: &JsonValue) -> Result<String, String> {
    if params.is_null() {
        return Ok(sql.to_string());
    }
    if let Some(v2) = params.as_object() {
        if v2.get("version").and_then(|v| v.as_i64()) == Some(2) {
            return apply_params_v2(sql, params);
        }
        if !v2.is_empty() {
            return apply_params_legacy_object(sql, v2);
        }
        return Ok(sql.to_string());
    }
    if let JsonValue::Array(arr) = params {
        return apply_params_legacy_array(sql, arr);
    }
    Ok(sql.to_string())
}

struct OccurrenceV2 {
    from: usize,
    to: usize,
    id: String,
    token: String,
}

fn apply_params_v2(sql: &str, params: &JsonValue) -> Result<String, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "Invalid v2 params payload".to_string())?;
    let values = obj
        .get("values")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "v2 params missing values object".to_string())?;
    let occs_val = obj
        .get("occurrences")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "v2 params missing occurrences array".to_string())?;

    let mut occurrences = Vec::with_capacity(occs_val.len());
    for item in occs_val {
        let occ_obj = item
            .as_object()
            .ok_or_else(|| "Invalid occurrence entry".to_string())?;
        let from = occ_obj
            .get("from")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "Occurrence missing from".to_string())? as usize;
        let to = occ_obj
            .get("to")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "Occurrence missing to".to_string())? as usize;
        let id = occ_obj
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Occurrence missing id".to_string())?
            .to_string();
        let token = occ_obj
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Occurrence missing token".to_string())?
            .to_string();
        occurrences.push(OccurrenceV2 {
            from,
            to,
            id,
            token,
        });
    }

    validate_occurrences(sql, &occurrences)?;
    let blocked = non_replaceable_ranges(sql);
    for occ in &occurrences {
        if is_in_non_replaceable(occ.from, occ.to, &blocked) {
            return Err(format!(
                "Parameter occurrence '{}' is inside a non-replaceable region",
                occ.token
            ));
        }
        if !values.contains_key(&occ.id) {
            return Err(format!("Missing value for parameter '{}'", occ.id));
        }
    }

    let mut result: Vec<char> = sql.chars().collect();
    let mut sorted = occurrences;
    sorted.sort_by(|a, b| b.from.cmp(&a.from));

    for occ in sorted {
        let lit = json_to_sql_literal(
            values
                .get(&occ.id)
                .ok_or_else(|| format!("Missing value for parameter '{}'", occ.id))?,
        );
        let replacement: Vec<char> = lit.chars().collect();
        result.splice(occ.from..occ.to, replacement);
    }
    Ok(result.into_iter().collect())
}

fn validate_occurrences(sql: &str, occurrences: &[OccurrenceV2]) -> Result<(), String> {
    let char_len = sql.chars().count();
    let mut prev_to = 0;
    for occ in occurrences {
        if occ.from >= occ.to || occ.to > char_len {
            return Err(format!(
                "Invalid occurrence span {}..{} for SQL length {char_len}",
                occ.from, occ.to
            ));
        }
        if occ.from < prev_to {
            return Err("Occurrence spans must be ordered and non-overlapping".to_string());
        }
        let actual: String = sql.chars().skip(occ.from).take(occ.to - occ.from).collect();
        if actual != occ.token {
            return Err(format!(
                "Token mismatch at span {}..{}: expected '{}', got '{actual}'",
                occ.from, occ.to, occ.token
            ));
        }
        prev_to = occ.to;
    }
    Ok(())
}

fn apply_params_legacy_object(
    sql: &str,
    map: &serde_json::Map<String, JsonValue>,
) -> Result<String, String> {
    let mut out = sql.to_string();
    let mut names: Vec<&String> = map.keys().collect();
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for name in names {
        let lit = json_to_sql_literal(&map[name]);
        out = replace_placeholder(&out, &format!(":{name}"), &lit);
        out = replace_placeholder(&out, &format!("${name}"), &lit);
    }
    Ok(out)
}

fn apply_params_legacy_array(sql: &str, arr: &[JsonValue]) -> Result<String, String> {
    let mut out = sql.to_string();
    for (i, value) in arr.iter().enumerate() {
        let lit = json_to_sql_literal(value);
        out = replace_next_question(&out, &lit)?;
        let positional = format!("${}", i + 1);
        out = replace_placeholder(&out, &positional, &lit);
    }
    Ok(out)
}

fn json_to_sql_literal(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "NULL".into(),
        JsonValue::Bool(true) => "TRUE".into(),
        JsonValue::Bool(false) => "FALSE".into(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('\'', "''");
            format!("'{escaped}'")
        }
        other => {
            let raw = other.to_string();
            let escaped = raw.replace('\\', "\\\\").replace('\'', "''");
            format!("'{escaped}'")
        }
    }
}

fn replace_next_question(sql: &str, lit: &str) -> Result<String, String> {
    let mut out = String::with_capacity(sql.len() + lit.len());
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    let mut replaced = false;
    while i < chars.len() {
        if let Some((consumed, _)) = skip_quoted(&chars, i) {
            out.extend(&chars[i..i + consumed]);
            i += consumed;
            continue;
        }
        if chars[i] == '?' && !replaced {
            out.push_str(lit);
            replaced = true;
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    if !replaced {
        return Ok(sql.to_string());
    }
    Ok(out)
}

fn replace_placeholder(sql: &str, placeholder: &str, lit: &str) -> String {
    let needle: Vec<char> = placeholder.chars().collect();
    if needle.is_empty() {
        return sql.to_string();
    }
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len() + lit.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some((consumed, _)) = skip_quoted(&chars, i) {
            out.extend(&chars[i..i + consumed]);
            i += consumed;
            continue;
        }
        if chars[i..].starts_with(&needle) {
            let after = i + needle.len();
            let boundary_ok = after >= chars.len()
                || !(chars[after].is_ascii_alphanumeric() || chars[after] == '_');
            if boundary_ok {
                out.push_str(lit);
                i = after;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn v2_named_substitution() {
        let sql = apply_params(
            "SELECT * FROM t WHERE id = :uid AND name = :name",
            &json!({
                "version": 2,
                "values": { "named:uid": 42, "named:name": "O'Brien" },
                "occurrences": [
                    { "from": 27, "to": 31, "id": "named:uid", "token": ":uid" },
                    { "from": 43, "to": 48, "id": "named:name", "token": ":name" }
                ]
            }),
        )
        .unwrap();
        assert_eq!(sql, "SELECT * FROM t WHERE id = 42 AND name = 'O''Brien'");
    }

    #[test]
    fn v2_template_substitution() {
        let sql = apply_params(
            "SELECT ${id}",
            &json!({
                "version": 2,
                "values": { "named:id": 7 },
                "occurrences": [{ "from": 7, "to": 12, "id": "named:id", "token": "${id}" }]
            }),
        )
        .unwrap();
        assert_eq!(sql, "SELECT 7");
    }

    #[test]
    fn v2_question_substitution() {
        let sql = apply_params(
            "SELECT ? , ?",
            &json!({
                "version": 2,
                "values": { "question:1": true, "question:2": false },
                "occurrences": [
                    { "from": 7, "to": 8, "id": "question:1", "token": "?" },
                    { "from": 11, "to": 12, "id": "question:2", "token": "?" }
                ]
            }),
        )
        .unwrap();
        assert_eq!(sql, "SELECT TRUE , FALSE");
    }

    #[test]
    fn v2_rejects_overlapping_spans() {
        let err = apply_params(
            "SELECT :a",
            &json!({
                "version": 2,
                "values": { "named:a": 1 },
                "occurrences": [
                    { "from": 7, "to": 9, "id": "named:a", "token": ":a" },
                    { "from": 8, "to": 9, "id": "named:a", "token": "a" }
                ]
            }),
        )
        .unwrap_err();
        assert!(err.contains("non-overlapping"));
    }

    #[test]
    fn v2_rejects_token_mismatch() {
        let err = apply_params(
            "SELECT :a",
            &json!({
                "version": 2,
                "values": { "named:a": 1 },
                "occurrences": [{ "from": 7, "to": 9, "id": "named:a", "token": ":b" }]
            }),
        )
        .unwrap_err();
        assert!(err.contains("Token mismatch"));
    }

    #[test]
    fn legacy_object_still_works() {
        let sql = apply_params("SELECT :uid", &json!({ "uid": 1 })).unwrap();
        assert_eq!(sql, "SELECT 1");
    }

    #[test]
    fn legacy_array_still_works() {
        let sql = apply_params("SELECT $1 AND $2", &json!([1, "x"])).unwrap();
        assert_eq!(sql, "SELECT 1 AND 'x'");
    }
}
