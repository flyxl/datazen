//! Connection-level SQL safety: read-only connections and Safe Mode.

use serde_json::Value as JsonValue;

const WRITE_VERBS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "MERGE", "UPSERT", "REPLACE", "CREATE", "ALTER", "DROP",
    "TRUNCATE", "GRANT", "REVOKE", "RENAME", "COPY", "LOAD", "UNLOAD", "CALL", "EXEC", "EXECUTE",
    "DO", "HANDLER", "OPTIMIZE", "REPAIR", "FLUSH", "RESET", "KILL", "SHUTDOWN", "PURGE", "VACUUM",
    "LOCK", "UNLOCK", "COMMENT",
];

const SAFE_MODE_NEEDS_WHERE: &[&str] = &["UPDATE", "DELETE"];
const SAFE_MODE_BLOCKED: &[&str] = &["TRUNCATE", "DROP"];

/// Reject mutating SQL when the connection is read-only, and require WHERE
/// for UPDATE/DELETE when Safe Mode is on. Safe Mode also blocks TRUNCATE/DROP.
pub fn check_sql(sql: &str, read_only: bool, safe_mode: bool) -> Result<(), String> {
    if !read_only && !safe_mode {
        return Ok(());
    }
    for stmt in split_statements(sql) {
        let Some(verb) = crate::mcp::permission::sql_main_keyword(&stmt) else {
            continue;
        };
        if read_only && is_write_verb(&verb) {
            return Err(format!(
                "Connection is read-only; '{verb}' statements are not allowed"
            ));
        }
        if safe_mode
            && SAFE_MODE_BLOCKED
                .iter()
                .any(|v| verb.eq_ignore_ascii_case(v))
        {
            return Err(format!("Safe Mode blocks {verb}"));
        }
        if safe_mode
            && SAFE_MODE_NEEDS_WHERE
                .iter()
                .any(|v| verb.eq_ignore_ascii_case(v))
        {
            if !has_top_level_where(&stmt) {
                return Err(format!(
                    "Safe Mode requires a WHERE clause on {verb} statements"
                ));
            }
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub fn is_write_sql(sql: &str) -> bool {
    split_statements(sql).iter().any(|stmt| {
        crate::mcp::permission::sql_main_keyword(stmt)
            .map(|verb| is_write_verb(&verb))
            .unwrap_or(false)
    })
}

fn is_write_verb(verb: &str) -> bool {
    WRITE_VERBS.iter().any(|v| v.eq_ignore_ascii_case(verb))
}

/// Substitute `:name` / `$1` placeholders with JSON param values as SQL literals.
/// Placeholders inside quoted strings are left untouched.
pub fn apply_params(sql: &str, params: &JsonValue) -> Result<String, String> {
    if params.is_null() {
        return Ok(sql.to_string());
    }
    let map = match params {
        JsonValue::Object(m) if !m.is_empty() => m,
        JsonValue::Array(arr) => {
            let mut out = sql.to_string();
            // Replace `?` left-to-right, then `$1`..`$n`.
            for (i, value) in arr.iter().enumerate() {
                let lit = json_to_sql_literal(value);
                out = replace_next_question(&out, &lit)?;
                let positional = format!("${}", i + 1);
                out = replace_placeholder(&out, &positional, &lit);
            }
            return Ok(out);
        }
        _ => return Ok(sql.to_string()),
    };

    let mut out = sql.to_string();
    let mut names: Vec<&String> = map.keys().collect();
    // Longer names first so `:user_id` wins over `:user`.
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for name in names {
        let lit = json_to_sql_literal(&map[name]);
        out = replace_placeholder(&out, &format!(":{name}"), &lit);
        out = replace_placeholder(&out, &format!("${name}"), &lit);
    }
    Ok(out)
}

fn json_to_sql_literal(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "NULL".into(),
        JsonValue::Bool(true) => "TRUE".into(),
        JsonValue::Bool(false) => "FALSE".into(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => format!("'{}'", s.replace('\'', "''")),
        other => format!("'{}'", other.to_string().replace('\'', "''")),
    }
}

fn replace_next_question(sql: &str, lit: &str) -> Result<String, String> {
    let mut out = String::with_capacity(sql.len() + lit.len());
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    let mut replaced = false;
    while i < chars.len() {
        let ch = chars[i];
        if let Some((consumed, _)) = skip_quoted(&chars, i) {
            out.extend(&chars[i..i + consumed]);
            i += consumed;
            continue;
        }
        if ch == '?' && !replaced {
            out.push_str(lit);
            replaced = true;
            i += 1;
            continue;
        }
        out.push(ch);
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

fn skip_quoted(chars: &[char], i: usize) -> Option<(usize, char)> {
    let quote = chars[i];
    if quote != '\'' && quote != '"' && quote != '`' {
        return None;
    }
    let mut j = i + 1;
    while j < chars.len() {
        if chars[j] == quote {
            if quote == '\'' && j + 1 < chars.len() && chars[j + 1] == '\'' {
                j += 2;
                continue;
            }
            return Some((j - i + 1, quote));
        }
        j += 1;
    }
    Some((chars.len() - i, quote))
}

fn split_statements(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut stmts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        if let Some((consumed, _)) = skip_quoted(&chars, i) {
            i += consumed;
            continue;
        }
        if chars[i] == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        if chars[i] == ';' {
            let chunk: String = chars[start..i].iter().collect();
            if !chunk.trim().is_empty() {
                stmts.push(chunk);
            }
            start = i + 1;
        }
        i += 1;
    }
    let chunk: String = chars[start..].iter().collect();
    if !chunk.trim().is_empty() {
        stmts.push(chunk);
    }
    stmts
}

fn has_top_level_where(sql: &str) -> bool {
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    let mut depth = 0;
    let mut ident = String::new();
    while i < chars.len() {
        if let Some((consumed, _)) = skip_quoted(&chars, i) {
            ident.clear();
            i += consumed;
            continue;
        }
        let ch = chars[i];
        if ch == '(' {
            depth += 1;
            ident.clear();
        } else if ch == ')' && depth > 0 {
            depth -= 1;
            ident.clear();
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            ident.push(ch);
        } else {
            if depth == 0 && ident.eq_ignore_ascii_case("WHERE") {
                return true;
            }
            ident.clear();
        }
        i += 1;
    }
    depth == 0 && ident.eq_ignore_ascii_case("WHERE")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn read_only_blocks_update() {
        let err = check_sql("UPDATE t SET x = 1 WHERE id = 1", true, false).unwrap_err();
        assert!(err.contains("read-only"));
    }

    #[test]
    fn read_only_allows_select() {
        assert!(check_sql("SELECT * FROM t", true, true).is_ok());
        assert!(check_sql("WITH c AS (SELECT 1) SELECT * FROM c", true, true).is_ok());
    }

    #[test]
    fn safe_mode_blocks_update_without_where() {
        let err = check_sql("UPDATE users SET name = 'x'", false, true).unwrap_err();
        assert!(err.contains("WHERE"));
    }

    #[test]
    fn safe_mode_allows_update_with_where() {
        assert!(check_sql("UPDATE users SET name = 'x' WHERE id = 1", false, true).is_ok());
    }

    #[test]
    fn safe_mode_ignores_where_inside_subquery() {
        let err = check_sql(
            "UPDATE t SET x = (SELECT y FROM u WHERE id = 1)",
            false,
            true,
        )
        .unwrap_err();
        assert!(err.contains("WHERE"));
    }

    #[test]
    fn safe_mode_blocks_delete_without_where() {
        assert!(check_sql("DELETE FROM t", false, true).is_err());
        assert!(check_sql("DELETE FROM t WHERE id = 1", false, true).is_ok());
    }

    #[test]
    fn safe_mode_blocks_truncate() {
        let err = check_sql("TRUNCATE TABLE t", false, true).unwrap_err();
        assert!(err.contains("TRUNCATE"));
    }

    #[test]
    fn safe_mode_blocks_drop() {
        for sql in [
            "DROP TABLE t",
            "DROP VIEW v",
            "DROP INDEX idx",
            "DROP TABLE IF EXISTS t",
        ] {
            let err = check_sql(sql, false, true).unwrap_err();
            assert!(err.contains("DROP"), "expected DROP block for {sql}: {err}");
        }
        assert!(check_sql("DROP TABLE t", false, false).is_ok());
    }

    #[test]
    fn named_params_are_substituted() {
        let sql = apply_params(
            "SELECT * FROM t WHERE id = :uid AND name = :name",
            &json!({ "uid": 42, "name": "O'Brien" }),
        )
        .unwrap();
        assert_eq!(sql, "SELECT * FROM t WHERE id = 42 AND name = 'O''Brien'");
    }

    #[test]
    fn params_inside_strings_are_not_replaced() {
        let sql =
            apply_params("SELECT ':uid' FROM t WHERE id = :uid", &json!({ "uid": 1 })).unwrap();
        assert_eq!(sql, "SELECT ':uid' FROM t WHERE id = 1");
    }

    #[test]
    fn positional_dollar_params() {
        let sql =
            apply_params("SELECT * FROM t WHERE a = $1 AND b = $2", &json!([1, "x"])).unwrap();
        assert_eq!(sql, "SELECT * FROM t WHERE a = 1 AND b = 'x'");
    }

    #[test]
    fn disabled_guards_allow_writes() {
        assert!(check_sql("UPDATE t SET x = 1", false, false).is_ok());
        assert!(check_sql("DELETE FROM t", false, false).is_ok());
        assert!(check_sql("DROP TABLE t", false, false).is_ok());
        assert!(check_sql("TRUNCATE TABLE t", false, false).is_ok());
    }

    #[test]
    fn read_only_blocks_grant_and_ddl() {
        assert!(check_sql("GRANT SELECT ON t TO u", true, false).is_err());
        assert!(check_sql("CREATE TABLE t (id int)", true, false).is_err());
        assert!(check_sql("DROP TABLE t", true, false).is_err());
    }

    #[test]
    fn safe_mode_allows_insert_and_grant_without_where() {
        assert!(check_sql("INSERT INTO t VALUES (1)", false, true).is_ok());
        assert!(check_sql("GRANT SELECT ON t TO u", false, true).is_ok());
    }

    #[test]
    fn comments_and_strings_are_not_statements() {
        assert!(check_sql("-- UPDATE t SET x = 1\nSELECT 1", true, true).is_ok());
        assert!(check_sql("SELECT 'UPDATE t' FROM dual", true, true).is_ok());
        assert!(check_sql("/* DELETE FROM t */ SELECT 1", true, true).is_ok());
    }

    #[test]
    fn where_at_end_of_statement_counts() {
        assert!(check_sql("DELETE FROM t WHERE id = 1", false, true).is_ok());
        assert!(check_sql("UPDATE t SET x = 1 WHERE", false, true).is_ok());
    }

    #[test]
    fn is_write_sql_detects_verbs() {
        assert!(is_write_sql("UPDATE t SET x = 1"));
        assert!(!is_write_sql("SELECT 1"));
        assert!(!is_write_sql("-- comment only"));
    }

    #[test]
    fn apply_params_handles_null_bool_and_question_marks() {
        assert_eq!(
            apply_params("SELECT :x", &json!(null)).unwrap(),
            "SELECT :x"
        );
        assert_eq!(apply_params("SELECT :x", &json!({})).unwrap(), "SELECT :x");
        let sql = apply_params("SELECT ? , ? , ?", &json!([true, false, {"a": 1}])).unwrap();
        assert_eq!(sql, "SELECT TRUE , FALSE , '{\"a\":1}'");
    }

    #[test]
    fn longer_named_params_win() {
        let sql = apply_params(
            "SELECT :user_id, :user",
            &json!({ "user": "a", "user_id": 9 }),
        )
        .unwrap();
        assert_eq!(sql, "SELECT 9, 'a'");
    }

    #[test]
    fn named_digit_keys_also_replace_dollar() {
        let sql = apply_params("SELECT :1, $1", &json!({ "1": 7 })).unwrap();
        assert_eq!(sql, "SELECT 7, 7");
    }

    #[test]
    fn extra_question_marks_are_left_alone() {
        let sql = apply_params("SELECT ? , ?", &json!([1])).unwrap();
        assert_eq!(sql, "SELECT 1 , ?");
    }

    #[test]
    fn split_respects_semicolons_inside_strings() {
        assert!(check_sql("SELECT 'a; UPDATE t'; SELECT 1", true, true).is_ok());
    }

    #[test]
    fn read_only_blocks_insert_and_delete() {
        assert!(check_sql("INSERT INTO t VALUES (1)", true, false).is_err());
        assert!(check_sql("DELETE FROM t WHERE id = 1", true, false).is_err());
    }

    #[test]
    fn mixed_statements_read_only_blocks_any_write() {
        let err = check_sql("SELECT 1; UPDATE t SET x = 1 WHERE id = 1", true, false).unwrap_err();
        assert!(err.contains("read-only"));
        let err = check_sql("SELECT 1; INSERT INTO t VALUES (1)", true, false).unwrap_err();
        assert!(err.contains("read-only"));
    }

    #[test]
    fn mixed_statements_safe_mode_blocks_drop_and_truncate() {
        for sql in [
            "SELECT 1; DROP TABLE t",
            "SELECT 1; TRUNCATE TABLE t",
            "DROP TABLE t; SELECT 1",
        ] {
            let err = check_sql(sql, false, true).unwrap_err();
            assert!(
                err.contains("DROP") || err.contains("TRUNCATE"),
                "expected block for {sql}: {err}"
            );
        }
    }

    #[test]
    fn mixed_statements_safe_mode_blocks_update_without_where() {
        let err = check_sql("SELECT 1; UPDATE t SET x = 1", false, true).unwrap_err();
        assert!(err.contains("WHERE"));
        let err = check_sql("DELETE FROM t; SELECT 1", false, true).unwrap_err();
        assert!(err.contains("WHERE"));
    }

    #[test]
    fn mixed_statements_safe_mode_allows_safe_writes() {
        assert!(check_sql("SELECT 1; SELECT 2", false, true).is_ok());
        assert!(check_sql(
            "SELECT 1; UPDATE t SET x = 1 WHERE id = 1; DELETE FROM t WHERE id = 2",
            false,
            true
        )
        .is_ok());
    }

    #[test]
    fn read_only_and_safe_mode_both_apply() {
        assert!(check_sql("INSERT INTO t VALUES (1)", true, true).is_err());
        assert!(check_sql("DROP TABLE t", true, true).is_err());
        assert!(check_sql("UPDATE t SET x = 1", true, true).is_err());
    }

    // --- [tester] edge-case / bypass probes ---

    #[test]
    fn test_tester_inline_block_comment_inside_drop_is_heuristic_gap() {
        // Tokenizer strips inline comments; verb becomes TABLE — known bypass.
        assert!(check_sql("DROP/**/TABLE t", false, true).is_ok());
        assert!(check_sql("TRUNCATE/**/TABLE t", false, true).is_ok());
    }

    #[test]
    fn test_tester_nested_block_comment_with_drop_still_blocked() {
        assert!(check_sql("/* outer /* inner */ DROP TABLE t */", false, true).is_err());
    }

    #[test]
    fn test_tester_drop_only_in_leading_comment_is_allowed() {
        assert!(check_sql("/* DROP TABLE t */ SELECT 1", false, true).is_ok());
        assert!(check_sql("-- DROP TABLE t\nSELECT 1", false, true).is_ok());
    }

    #[test]
    fn test_tester_read_only_inline_comment_drop_is_heuristic_gap() {
        assert!(check_sql("DROP/**/TABLE t", true, false).is_ok());
    }

    #[test]
    fn test_tester_unicode_fullwidth_drop_bypasses_safe_mode() {
        // Fullwidth Latin letters are not recognized as DROP — documented heuristic gap.
        assert!(check_sql("ＤＲＯＰ TABLE t", false, true).is_ok());
    }

    #[test]
    fn test_tester_null_byte_splits_drop_keyword_is_heuristic_gap() {
        assert!(check_sql("DROP\u{0000}TABLE t", false, true).is_ok());
    }

    #[test]
    fn test_tester_control_chars_in_select_do_not_crash() {
        assert!(check_sql("SELECT\u{0001}1", true, true).is_ok());
    }

    #[test]
    fn test_tester_mixed_case_and_extra_whitespace_drop_blocked() {
        assert!(check_sql("  \n  DrOp  \t  TaBlE t", false, true).is_err());
        assert!(check_sql("  truncate   table   t  ", false, true).is_err());
    }

    #[test]
    fn test_tester_drop_keyword_split_across_comment_is_heuristic_gap() {
        // Verb becomes TABLE after comment strip — known bypass, not a formal guarantee.
        assert!(check_sql("/* DROP */ TABLE t", false, true).is_ok());
    }
}
