//! SQL safety guards: read-only connections and Safe Mode.

use super::keyword::{find_main_verb_index, tokenize_sql};
use super::scanner::{push_separator, skip_dollar_quoted, skip_quoted, split_statements};

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

fn matching_close_paren(tokens: &[String], open: usize) -> Option<usize> {
    let mut depth: usize = 0;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.as_str() {
            "(" => depth += 1,
            ")" => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Detect data-modifying CTE bodies, including multiple CTEs and PostgreSQL's
/// optional `MATERIALIZED` modifiers. Scanning every `AS (...)` also finds a
/// nested `WITH` inside a subquery; a valid SQL column alias cannot be followed
/// directly by a parenthesized expression.
fn has_data_modifying_cte(tokens: &[String]) -> bool {
    for (index, token) in tokens.iter().enumerate() {
        if !token.eq_ignore_ascii_case("AS") {
            continue;
        }
        let mut open = index + 1;
        if tokens
            .get(open)
            .is_some_and(|token| token.eq_ignore_ascii_case("NOT"))
        {
            open += 1;
            if tokens
                .get(open)
                .is_some_and(|token| token.eq_ignore_ascii_case("MATERIALIZED"))
            {
                open += 1;
            }
        } else if tokens
            .get(open)
            .is_some_and(|token| token.eq_ignore_ascii_case("MATERIALIZED"))
        {
            open += 1;
        }
        if tokens.get(open).map(String::as_str) != Some("(") {
            continue;
        }
        let Some(close) = matching_close_paren(tokens, open) else {
            continue;
        };
        let body = &tokens[open + 1..close];
        let Some(main) = find_main_verb_index(body) else {
            continue;
        };
        if is_write_verb(&body[main]) {
            return true;
        }
    }
    false
}

/// Returns the statement executed by `EXPLAIN ANALYZE`, accounting for both
/// the shorthand and PostgreSQL's parenthesized option syntax. `ANALYZE FALSE`
/// is an ordinary plan and must not be treated as executing the query.
///
/// The shorthand accepts the option keywords in either order — PostgreSQL's
/// grammar is `EXPLAIN opt_verbose opt_analyze`, and `EXPLAIN VERBOSE ANALYZE
/// ...` is valid too. The option run is skipped as a whole before the
/// statement starts: classifying the statement by its first surviving token
/// would let `EXPLAIN ANALYZE VERBOSE DELETE ...` be misread as a plan-only
/// `EXPLAIN` (first surviving token `VERBOSE`, not a write verb), which
/// executes the write on read-only connections — a fail-open hole. A form
/// without `ANALYZE`/`ANALYSE` still resolves to `None`, so plan-only
/// `EXPLAIN DELETE` (nothing executes) remains allowed.
fn explain_analyze_statement(tokens: &[String]) -> Option<&[String]> {
    if !tokens.first()?.eq_ignore_ascii_case("EXPLAIN") {
        return None;
    }

    if tokens.get(1).map(String::as_str) == Some("(") {
        let close = matching_close_paren(tokens, 1)?;
        let options = &tokens[2..close];
        let analyze = options
            .iter()
            .position(|token| is_analyze_option_keyword(token))?;
        if analyse_bool_is_false(options, analyze) {
            return None;
        }
        return Some(&tokens[close + 1..]);
    }

    let mut saw_analyze = false;
    let mut statement = 1;
    while let Some(token) = tokens.get(statement) {
        let value = token.as_str();
        if is_analyze_option_keyword(value) {
            saw_analyze = true;
            let (next, is_false) = skip_analyze_bool(tokens, statement + 1);
            if is_false {
                return None;
            }
            statement = next;
        } else if value.eq_ignore_ascii_case("VERBOSE") {
            statement += 1;
        } else {
            break;
        }
    }
    if !saw_analyze {
        return None;
    }
    Some(&tokens[statement..])
}

/// PostgreSQL accepts both spellings (`ANALYZE` / `ANALYSE`).
fn is_analyze_option_keyword(token: &str) -> bool {
    token.eq_ignore_ascii_case("ANALYZE") || token.eq_ignore_ascii_case("ANALYSE")
}

/// Consumes the optional `=` and boolean starting at `index` (right after an
/// ANALYZE keyword) and reports whether that boolean turns analysis off
/// (`ANALYZE FALSE`). A bare `ANALYZE` without a boolean means "on", so it
/// reports `false` while consuming nothing.
fn skip_analyze_bool(tokens: &[String], index: usize) -> (usize, bool) {
    let mut value = index;
    if tokens.get(value).map(String::as_str) == Some("=") {
        value += 1;
    }
    if tokens
        .get(value)
        .is_some_and(|token| is_explicit_bool(token))
    {
        (value + 1, is_false_value(&tokens[value]))
    } else {
        (index, false)
    }
}

fn is_explicit_bool(token: &str) -> bool {
    ["TRUE", "FALSE", "ON", "OFF", "YES", "NO", "1", "0"]
        .iter()
        .any(|value| token.eq_ignore_ascii_case(value))
}

fn is_false_value(token: &str) -> bool {
    ["FALSE", "OFF", "NO", "0"]
        .iter()
        .any(|value| token.eq_ignore_ascii_case(value))
}

fn analyse_bool_is_false(options: &[String], analyze_index: usize) -> bool {
    skip_analyze_bool(options, analyze_index + 1).1
}

fn has_write_operation(tokens: &[String]) -> bool {
    let has_main_write = find_main_verb_index(tokens)
        .and_then(|index| tokens.get(index))
        .is_some_and(|verb| is_write_verb(verb));
    has_main_write || has_data_modifying_cte(tokens) || has_select_into(tokens)
}

fn has_select_into(tokens: &[String]) -> bool {
    let mut select_depths = Vec::new();
    let mut depth: usize = 0;
    for token in tokens {
        match token.as_str() {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            _ if token.eq_ignore_ascii_case("SELECT") => {
                select_depths.resize(select_depths.len().max(depth + 1), false);
                select_depths[depth] = true;
            }
            _ if token.eq_ignore_ascii_case("INTO")
                && select_depths.get(depth).copied().unwrap_or(false) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
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
        let tokens = tokenize_sql(&stripped);
        let Some(main_index) = find_main_verb_index(&tokens) else {
            continue;
        };
        let Some(verb) = tokens.get(main_index).map(|verb| verb.to_uppercase()) else {
            continue;
        };
        if read_only && is_write_verb(&verb) {
            return Err(format!(
                "Connection is read-only; '{verb}' statements are not allowed"
            ));
        }
        if read_only {
            let executable = if verb.eq_ignore_ascii_case("EXPLAIN") {
                explain_analyze_statement(&tokens)
            } else {
                Some(tokens.as_slice())
            };
            if executable.is_some_and(has_write_operation) {
                return Err(
                    "Connection is read-only; data-modifying statements are not allowed".into(),
                );
            }
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
        let tokens = tokenize_sql(&stripped);
        has_write_operation(&tokens)
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
