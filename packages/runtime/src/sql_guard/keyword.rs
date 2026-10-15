//! SQL main-keyword classification: lexical tokenizer + main-verb detection.
//!
//! Extracted verbatim from the host MCP permission layer so that the shared runtime
//! kernel owns SQL statement classification for the desktop host and any future
//! server-side entry alike.

use super::safety::normalize_fullwidth;

/// First-keyword verbs treated as the main statement after an optional `WITH` clause.
const MAIN_VERBS: &[&str] = &[
    "SELECT",
    "INSERT",
    "UPDATE",
    "DELETE",
    "MERGE",
    "UPSERT",
    "REPLACE",
    "CALL",
    "EXEC",
    "EXECUTE",
    "DO",
    "SHOW",
    "DESCRIBE",
    "DESC",
    "EXPLAIN",
    "SET",
    "USE",
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT",
    "RELEASE",
    "PREPARE",
    "DEALLOCATE",
    "ANALYZE",
    "VACUUM",
    "COPY",
    "LOAD",
    "UNLOAD",
    "HANDLER",
    "OPTIMIZE",
    "REPAIR",
    "CHECKSUM",
    "CHECK",
    "FLUSH",
    "RESET",
    "KILL",
    "SHUTDOWN",
    "CREATE",
    "ALTER",
    "DROP",
    "TRUNCATE",
    "GRANT",
    "REVOKE",
    "RENAME",
    "COMMENT",
    "LOCK",
    "UNLOCK",
    "PURGE",
];

/// Returns the uppercase main SQL verb (handles leading `WITH` CTEs).
pub fn sql_main_keyword(sql: &str) -> Option<String> {
    // Normalise fullwidth characters so that ＤＲＯＰ → DROP before classification.
    let sql = normalize_fullwidth(sql);
    let tokens = tokenize_sql(&sql);
    if tokens.is_empty() {
        return None;
    }

    let mut idx = 0;
    if tokens[idx].eq_ignore_ascii_case("WITH") {
        idx = skip_with_clause(&tokens)?;
    }

    Some(tokens[idx].to_uppercase())
}

/// Byte-level comment stripper used by [`tokenize_sql`].
///
/// Not the same function as [`super::safety::strip_sql_comments`]: this one is
/// quote-agnostic on purpose (the tokenizer re-parses string literals itself).
pub fn strip_sql_comments(sql: &str) -> String {
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

pub fn tokenize_sql(sql: &str) -> Vec<String> {
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
    MAIN_VERBS.iter().any(|v| v.eq_ignore_ascii_case(token))
}

pub fn find_main_verb_index(tokens: &[String]) -> Option<usize> {
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
