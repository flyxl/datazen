//! SQL safety guards: read-only connections and Safe Mode.

use crate::sql_guard::scanner::{
    push_separator, skip_dollar_quoted, skip_quoted, split_statements,
};

/// Reject SQL containing null bytes — they can split keywords and bypass
/// classification (e.g. `DROP\0TABLE t`).
pub fn reject_null_bytes(sql: &str) -> Result<(), String> {
    if sql.contains('\0') {
        Err("SQL contains null bytes which are not allowed".to_string())
    } else {
        Ok(())
    }
}

/// Map fullwidth Latin characters (U+FF01 – U+FF5E) to their ASCII
/// equivalents so that `ＤＲＯＰ` becomes `DROP` before classification.
pub fn normalize_fullwidth(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    for ch in sql.chars() {
        let cp = ch as u32;
        if (0xFF01..=0xFF5E).contains(&cp) {
            out.push((cp - 0xFEE0) as u8 as char);
        } else {
            out.push(ch);
        }
    }
    out
}

/// Strip SQL comments (`--` line and `/* … */` block) without touching
/// quoted strings. Used by `check_sql` to re-classify a statement after
/// comment removal.
pub fn strip_sql_comments(sql: &str) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < chars.len() {
        let quote = chars[i];
        if quote == '\'' || quote == '"' || quote == '`' {
            let mut j = i + 1;
            while j < chars.len() {
                if chars[j] == quote {
                    if quote == '\'' && j + 1 < chars.len() && chars[j + 1] == '\'' {
                        j += 2;
                        continue;
                    }
                    break;
                }
                j += 1;
            }
            let end = if j < chars.len() { j } else { chars.len() - 1 };
            for ch in &chars[i..=end] {
                out.push(*ch);
            }
            i = if j < chars.len() { j + 1 } else { chars.len() };
            continue;
        }
        // # line comment (MySQL)
        if chars[i] == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            push_separator(&mut out);
            continue;
        }
        if chars[i] == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            push_separator(&mut out);
            continue;
        }
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            push_separator(&mut out);
            continue;
        }
        // PG dollar-quoted string — preserve as-is (string literal)
        if chars[i] == '$' {
            if let Some(end) = skip_dollar_quoted(&chars, i) {
                for ch in &chars[i..end] {
                    out.push(*ch);
                }
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Full guard normalisation: reject null bytes + map fullwidth → halfwidth.
pub fn normalize_sql(sql: &str) -> Result<String, String> {
    reject_null_bytes(sql)?;
    Ok(normalize_fullwidth(sql))
}

const WRITE_VERBS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "MERGE", "UPSERT", "REPLACE", "CREATE", "ALTER", "DROP",
    "TRUNCATE", "GRANT", "REVOKE", "RENAME", "COPY", "LOAD", "UNLOAD", "CALL", "EXEC", "EXECUTE",
    "DO", "HANDLER", "OPTIMIZE", "REPAIR", "FLUSH", "RESET", "KILL", "SHUTDOWN", "PURGE", "VACUUM",
    "LOCK", "UNLOCK", "COMMENT",
];

const SAFE_MODE_NEEDS_WHERE: &[&str] = &["UPDATE", "DELETE"];
const SAFE_MODE_BLOCKED: &[&str] = &["TRUNCATE", "DROP"];

const READ_VERBS: &[&str] = &[
    "SELECT", "SHOW", "DESCRIBE", "DESC", "EXPLAIN", "PRAGMA", "USE", "WITH", "VALUES",
];

fn is_read_verb(v: &str) -> bool {
    READ_VERBS.iter().any(|r| v.eq_ignore_ascii_case(r))
}

fn is_write_verb(verb: &str) -> bool {
    WRITE_VERBS.iter().any(|v| v.eq_ignore_ascii_case(verb))
}

/// True when a `/* … */` block comment in `sql` contains a write verb.
fn comment_hides_write_verb(sql: &str) -> bool {
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            let start = i + 2;
            let mut j = start;
            while j + 1 < chars.len() && !(chars[j] == '*' && chars[j + 1] == '/') {
                j += 1;
            }
            let comment_text: String = chars[start..j].iter().collect();
            let norm = normalize_fullwidth(&comment_text);
            if WRITE_VERBS.iter().any(|v| {
                norm.split_whitespace()
                    .any(|tok| tok.eq_ignore_ascii_case(v))
            }) {
                return true;
            }
            i = (j + 2).min(chars.len());
            continue;
        }
        i += 1;
    }
    false
}

/// Reject mutating SQL when the connection is read-only, and require WHERE
/// for UPDATE/DELETE when Safe Mode is on. Safe Mode also blocks TRUNCATE/DROP.
pub fn check_sql(sql: &str, read_only: bool, safe_mode: bool) -> Result<(), String> {
    if !read_only && !safe_mode {
        return Ok(());
    }
    let sql = normalize_sql(sql)?;
    for stmt in split_statements(&sql) {
        let stripped = strip_sql_comments(&stmt);
        let Some(verb) = crate::mcp::permission::sql_main_keyword(&stripped) else {
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
            if !has_top_level_where(&stripped) {
                return Err(format!(
                    "Safe Mode requires a WHERE clause on {verb} statements"
                ));
            }
        }
        if !is_write_verb(&verb)
            && !is_read_verb(&verb)
            && comment_hides_write_verb(&stmt)
            && (read_only || safe_mode)
        {
            return Err(format!(
                "Comment hides a write verb; '{verb}' statement is not allowed"
            ));
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub fn is_write_sql(sql: &str) -> bool {
    let Ok(sql) = normalize_sql(sql) else {
        return false;
    };
    split_statements(&sql).iter().any(|stmt| {
        let stripped = strip_sql_comments(stmt);
        crate::mcp::permission::sql_main_keyword(&stripped)
            .map(|verb| is_write_verb(&verb))
            .unwrap_or(false)
    })
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
        // PG dollar-quoted string — skip as string literal
        if let Some(end) = skip_dollar_quoted(&chars, i) {
            ident.clear();
            i = end;
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
