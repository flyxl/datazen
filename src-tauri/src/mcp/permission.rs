//! MCP permission modes — tool restrictions and lightweight SQL classification.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// MCP tool permission tier persisted in application settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpPermissionMode {
    ReadOnly,
    SafeWrite,
    HighRiskWrite,
}

impl Default for McpPermissionMode {
    fn default() -> Self {
        Self::SafeWrite
    }
}

/// Tools blocked entirely in read-only mode.
const READ_ONLY_BLOCKED_TOOLS: &[&str] = &["query", "run_workflow"];

/// First-keyword verbs treated as the main statement after an optional `WITH` clause.
const MAIN_VERBS: &[&str] = &[
    "SELECT", "INSERT", "UPDATE", "DELETE", "MERGE", "UPSERT", "REPLACE", "CALL", "EXEC",
    "EXECUTE", "DO", "SHOW", "DESCRIBE", "DESC", "EXPLAIN", "SET", "USE", "BEGIN", "COMMIT",
    "ROLLBACK", "SAVEPOINT", "RELEASE", "PREPARE", "DEALLOCATE", "ANALYZE", "VACUUM", "COPY",
    "LOAD", "UNLOAD", "HANDLER", "OPTIMIZE", "REPAIR", "CHECKSUM", "CHECK", "FLUSH", "RESET",
    "KILL", "SHUTDOWN", "CREATE", "ALTER", "DROP", "TRUNCATE", "GRANT", "REVOKE", "RENAME",
    "COMMENT", "LOCK", "UNLOCK", "PURGE",
];

/// `CREATE` targets blocked in safe-write mode (destructive / admin DDL).
const SAFE_WRITE_BLOCKED_CREATE_TARGETS: &[&str] = &[
    "USER", "ROLE", "DATABASE", "SCHEMA", "TABLE", "INDEX", "VIEW", "MATERIALIZED", "TRIGGER",
    "FUNCTION", "PROCEDURE", "SERVER", "EXTENSION", "SEQUENCE", "TYPE", "DOMAIN", "CATALOG",
];

/// Standalone keywords blocked in safe-write mode.
const SAFE_WRITE_BLOCKED_KEYWORDS: &[&str] = &["DROP", "TRUNCATE", "ALTER", "GRANT", "REVOKE"];

/// Returns the uppercase main SQL verb (handles leading `WITH` CTEs).
pub fn sql_main_keyword(sql: &str) -> Option<String> {
    let tokens = tokenize_sql(sql);
    if tokens.is_empty() {
        return None;
    }

    let mut idx = 0;
    if tokens[idx].eq_ignore_ascii_case("WITH") {
        idx = skip_with_clause(&tokens)?;
    }

    Some(tokens[idx].to_uppercase())
}

/// Returns `Ok(())` when SQL is allowed under the given mode.
pub fn check_sql_allowed(sql: &str, mode: McpPermissionMode) -> Result<(), String> {
    match mode {
        McpPermissionMode::HighRiskWrite => Ok(()),
        McpPermissionMode::ReadOnly => Err(
            "SQL execution is blocked in MCP read-only permission mode".to_string(),
        ),
        McpPermissionMode::SafeWrite => check_safe_write_sql(sql),
    }
}

fn check_safe_write_sql(sql: &str) -> Result<(), String> {
    if contains_multiple_statements(sql) {
        return Err("Multiple SQL statements are not allowed in MCP safe-write mode".to_string());
    }

    let keyword = sql_main_keyword(sql)
        .ok_or_else(|| "Could not classify SQL statement".to_string())?;

    if SAFE_WRITE_BLOCKED_KEYWORDS
        .iter()
        .any(|k| keyword.eq_ignore_ascii_case(k))
    {
        return Err(format!(
            "SQL keyword '{keyword}' is not allowed in MCP safe-write permission mode"
        ));
    }

    if keyword.eq_ignore_ascii_case("CREATE") {
        let tokens = tokenize_sql(sql);
        let create_idx = find_main_verb_index(&tokens).unwrap_or(0);
        let next = tokens
            .get(create_idx + 1)
            .map(|t| t.to_uppercase())
            .unwrap_or_default();
        if next == "OR" {
            // CREATE OR REPLACE ...
            if let Some(after) = tokens.get(create_idx + 3) {
                if SAFE_WRITE_BLOCKED_CREATE_TARGETS
                    .iter()
                    .any(|t| after.eq_ignore_ascii_case(t))
                {
                    return Err(format!(
                        "CREATE {next} {after} is not allowed in MCP safe-write permission mode"
                    ));
                }
            }
        } else if SAFE_WRITE_BLOCKED_CREATE_TARGETS
            .iter()
            .any(|t| next.eq_ignore_ascii_case(t))
        {
            return Err(format!(
                "CREATE {next} is not allowed in MCP safe-write permission mode"
            ));
        }
    }

    Ok(())
}

/// Returns `Ok(())` when the tool call is allowed under mode + user denylist.
pub fn check_tool_call(
    tool_name: &str,
    mode: McpPermissionMode,
    disabled_tools: &HashSet<String>,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<(), String> {
    if disabled_tools.contains(tool_name) {
        return Err(format!("Tool '{tool_name}' is disabled in DataZen settings"));
    }

    match mode {
        McpPermissionMode::ReadOnly => {
            if READ_ONLY_BLOCKED_TOOLS.contains(&tool_name) {
                return Err(format!(
                    "Tool '{tool_name}' is blocked in MCP read-only permission mode"
                ));
            }
        }
        McpPermissionMode::SafeWrite => {
            if tool_name == "query" {
                if let Some(args) = arguments {
                    if let Some(sql) = args.get("sql").and_then(|v| v.as_str()) {
                        check_safe_write_sql(sql)?;
                    }
                }
            }
        }
        McpPermissionMode::HighRiskWrite => {}
    }

    Ok(())
}

/// Whether a tool should appear in MCP `list_tools` for the given mode + denylist.
pub fn is_tool_listed(
    tool_name: &str,
    mode: McpPermissionMode,
    disabled_tools: &HashSet<String>,
) -> bool {
    if disabled_tools.contains(tool_name) {
        return false;
    }
    match mode {
        McpPermissionMode::ReadOnly => !READ_ONLY_BLOCKED_TOOLS.contains(&tool_name),
        McpPermissionMode::SafeWrite | McpPermissionMode::HighRiskWrite => true,
    }
}

fn strip_sql_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn tokenize_sql(sql: &str) -> Vec<String> {
    let cleaned = strip_sql_comments(sql);
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;

    for ch in cleaned.chars() {
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        if in_backtick {
            if ch == '`' {
                in_backtick = false;
            }
            continue;
        }

        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '`' => in_backtick = true,
            c if c.is_whitespace() || c == ';' || c == ',' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            '(' | ')' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                tokens.push(ch.to_string());
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn is_main_verb(token: &str) -> bool {
    MAIN_VERBS
        .iter()
        .any(|v| v.eq_ignore_ascii_case(token))
}

fn find_main_verb_index(tokens: &[String]) -> Option<usize> {
    if tokens.is_empty() {
        return None;
    }
    let mut idx = 0;
    if tokens[idx].eq_ignore_ascii_case("WITH") {
        idx = skip_with_clause(tokens)?;
    }
    Some(idx)
}

fn skip_with_clause(tokens: &[String]) -> Option<usize> {
    let mut paren_depth = 0;
    for (i, token) in tokens.iter().enumerate().skip(1) {
        match token.as_str() {
            "(" => paren_depth += 1,
            ")" if paren_depth > 0 => paren_depth -= 1,
            _ if paren_depth == 0 && is_main_verb(token) => return Some(i),
            _ => {}
        }
    }
    None
}

fn contains_multiple_statements(sql: &str) -> bool {
    let cleaned = strip_sql_comments(sql);
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut seen_statement = false;

    for ch in cleaned.chars() {
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        if in_backtick {
            if ch == '`' {
                in_backtick = false;
            }
            continue;
        }
        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '`' => in_backtick = true,
            ';' => {
                if seen_statement {
                    let rest = cleaned.split(';').nth_back(0).unwrap_or("").trim();
                    if !rest.is_empty() {
                        return true;
                    }
                }
                seen_statement = true;
            }
            c if !c.is_whitespace() => seen_statement = true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn default_mode_is_safe_write() {
        assert_eq!(McpPermissionMode::default(), McpPermissionMode::SafeWrite);
    }

    #[test]
    fn sql_main_keyword_simple_select() {
        assert_eq!(
            sql_main_keyword("SELECT 1"),
            Some("SELECT".to_string())
        );
    }

    #[test]
    fn sql_main_keyword_with_cte_select() {
        assert_eq!(
            sql_main_keyword("WITH cte AS (SELECT 1) SELECT * FROM cte"),
            Some("SELECT".to_string())
        );
    }

    #[test]
    fn sql_main_keyword_with_cte_delete() {
        assert_eq!(
            sql_main_keyword("WITH t AS (SELECT id FROM u) DELETE FROM u WHERE id IN (SELECT id FROM t)"),
            Some("DELETE".to_string())
        );
    }

    #[test]
    fn sql_main_keyword_strips_comments() {
        assert_eq!(
            sql_main_keyword("-- drop hint\nSELECT 1 /* alter */"),
            Some("SELECT".to_string())
        );
    }

    #[test]
    fn safe_write_allows_insert_update_delete() {
        assert!(check_sql_allowed("INSERT INTO t VALUES (1)", McpPermissionMode::SafeWrite).is_ok());
        assert!(check_sql_allowed("UPDATE t SET x = 1", McpPermissionMode::SafeWrite).is_ok());
        assert!(check_sql_allowed("DELETE FROM t", McpPermissionMode::SafeWrite).is_ok());
    }

    #[test]
    fn safe_write_blocks_drop_truncate_alter() {
        for sql in ["DROP TABLE t", "TRUNCATE t", "ALTER TABLE t ADD c INT"] {
            assert!(
                check_sql_allowed(sql, McpPermissionMode::SafeWrite).is_err(),
                "expected block: {sql}"
            );
        }
    }

    #[test]
    fn safe_write_blocks_create_user_and_grant() {
        assert!(check_sql_allowed("CREATE USER u", McpPermissionMode::SafeWrite).is_err());
        assert!(check_sql_allowed("CREATE ROLE r", McpPermissionMode::SafeWrite).is_err());
        assert!(check_sql_allowed("GRANT ALL ON t TO u", McpPermissionMode::SafeWrite).is_err());
        assert!(check_sql_allowed("REVOKE ALL ON t FROM u", McpPermissionMode::SafeWrite).is_err());
    }

    #[test]
    fn safe_write_blocks_create_table() {
        assert!(check_sql_allowed(
            "CREATE TABLE t (id INT)",
            McpPermissionMode::SafeWrite
        )
        .is_err());
    }

    #[test]
    fn high_risk_write_allows_ddl() {
        assert!(check_sql_allowed("DROP TABLE t", McpPermissionMode::HighRiskWrite).is_ok());
    }

    #[test]
    fn read_only_mode_blocks_sql() {
        assert!(check_sql_allowed("SELECT 1", McpPermissionMode::ReadOnly).is_err());
    }

    #[test]
    fn read_only_blocks_query_and_run_workflow() {
        let none = disabled(&[]);
        assert!(check_tool_call("query", McpPermissionMode::ReadOnly, &none, None).is_err());
        assert!(check_tool_call("run_workflow", McpPermissionMode::ReadOnly, &none, None).is_err());
        assert!(check_tool_call("list_tables", McpPermissionMode::ReadOnly, &none, None).is_ok());
    }

    #[test]
    fn safe_write_allows_query_with_select() {
        let none = disabled(&[]);
        let args = serde_json::json!({"sql": "SELECT 1"}).as_object().cloned();
        assert!(check_tool_call(
            "query",
            McpPermissionMode::SafeWrite,
            &none,
            args.as_ref()
        )
        .is_ok());
    }

    #[test]
    fn safe_write_blocks_query_with_drop() {
        let none = disabled(&[]);
        let args = serde_json::json!({"sql": "DROP TABLE t"}).as_object().cloned();
        assert!(check_tool_call(
            "query",
            McpPermissionMode::SafeWrite,
            &none,
            args.as_ref()
        )
        .is_err());
    }

    #[test]
    fn high_risk_write_allows_query_with_drop() {
        let none = disabled(&[]);
        let args = serde_json::json!({"sql": "DROP TABLE t"}).as_object().cloned();
        assert!(check_tool_call(
            "query",
            McpPermissionMode::HighRiskWrite,
            &none,
            args.as_ref()
        )
        .is_ok());
    }

    #[test]
    fn disabled_tools_apply_in_all_modes() {
        let disabled = disabled(&["list_tables"]);
        assert!(check_tool_call(
            "list_tables",
            McpPermissionMode::HighRiskWrite,
            &disabled,
            None
        )
        .is_err());
    }

    #[test]
    fn list_tools_respects_mode_and_denylist() {
        let none = disabled(&[]);
        assert!(!is_tool_listed("query", McpPermissionMode::ReadOnly, &none));
        assert!(is_tool_listed("list_tables", McpPermissionMode::ReadOnly, &none));

        let disabled = disabled(&["query"]);
        assert!(!is_tool_listed("query", McpPermissionMode::SafeWrite, &disabled));
        assert!(is_tool_listed("query", McpPermissionMode::SafeWrite, &none));
    }
}
