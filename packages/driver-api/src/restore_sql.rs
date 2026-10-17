//! Opt-in contract for restoring SQL statements on a retained ResourceProvider.
//! It deliberately excludes COPY/client protocols and embedded transaction control.
use crate::{sql_dump::is_comment_only_or_empty, DatabaseDriver, DriverError};

pub const CAPABILITY_KEY: &str = "retainedSqlRestoreV1";
pub const TRANSACTIONAL: &str = "transactionalDdl";
pub const IMPLICIT_COMMIT: &str = "implicitCommitDdl";

/// Preflight the entire script before any external write. The returned statements
/// use the driver's scanner, including dollar quotes and MySQL DELIMITER handling.
pub fn prepare(
    driver: &dyn DatabaseDriver,
    input: &str,
    profile: &str,
    single_transaction: bool,
    overwrite: bool,
    target: &crate::namespace::NamespaceTarget,
) -> Result<Vec<String>, DriverError> {
    if !matches!(profile, TRANSACTIONAL | IMPLICIT_COMMIT)
        || (single_transaction && profile != TRANSACTIONAL)
    {
        return Err(DriverError::InvalidConfig(
            "Restore transaction mode unsupported by retained resource".into(),
        ));
    }
    let mut scanner = driver.new_sql_scanner();
    let mut statements = scanner.push(input);
    statements.extend(scanner.finish());
    let mut prepared = Vec::new();
    for sql in statements {
        if executable_comment(&sql) {
            return Err(DriverError::InvalidConfig(
                "Executable restore comments are unsupported".into(),
            ));
        }
        if is_comment_only_or_empty(&sql) {
            continue;
        }
        let body = leading_body(&sql)?;
        let words = prefix_words(body)?;
        let first = words.first().map(String::as_str).unwrap_or_default();
        // This is a bounded restore format, not arbitrary client scripting.
        // SET (including executable MySQL comments), USE and transaction control
        // could redirect the frozen namespace or end the owned transaction.
        if !matches!(
            first,
            "CREATE" | "ALTER" | "DROP" | "INSERT" | "UPDATE" | "DELETE" | "TRUNCATE" | "COMMENT"
        ) || words.iter().any(|word| {
            matches!(
                word.as_str(),
                "DATABASE"
                    | "SCHEMA"
                    | "TABLESPACE"
                    | "SERVER"
                    | "ROLE"
                    | "USER"
                    | "SYSTEM"
                    | "EXTENSION"
                    | "PUBLICATION"
                    | "SUBSCRIPTION"
            )
        }) {
            return Err(DriverError::InvalidConfig(
                "Restore input contains unsupported SQL/client protocol or context control".into(),
            ));
        }
        crate::restore_sql_target::validate(&sql, profile, target, overwrite)?;
        if overwrite && first == "CREATE" {
            let relation_kind = words.get(1).map(String::as_str).unwrap_or_default();
            if matches!(relation_kind, "TABLE" | "VIEW") {
                let ident = crate::sql_dump::created_relation_ident(body).ok_or_else(|| {
                    DriverError::InvalidConfig(
                        "Restore overwrite requires an explicit relation identifier".into(),
                    )
                })?;
                prepared.push(format!("DROP {relation_kind} IF EXISTS {ident}"));
            }
        }
        prepared.push(sql);
    }
    if prepared.is_empty() {
        return Err(DriverError::InvalidConfig(
            "Restore input contains no supported statements".into(),
        ));
    }
    Ok(prepared)
}

fn leading_body(mut sql: &str) -> Result<&str, DriverError> {
    loop {
        sql = sql.trim_start();
        if let Some(rest) = sql.strip_prefix("--") {
            sql = rest
                .split_once('\n')
                .map(|(_, rest)| rest)
                .unwrap_or_default();
        } else if let Some(rest) = sql.strip_prefix("/*") {
            if rest.starts_with('!') || rest.starts_with("M!") {
                return Err(DriverError::InvalidConfig(
                    "Executable restore comments are unsupported".into(),
                ));
            }
            sql = rest
                .split_once("*/")
                .map(|(_, rest)| rest)
                .ok_or_else(|| DriverError::InvalidConfig("Unterminated restore comment".into()))?;
        } else {
            return Ok(sql);
        }
    }
}

fn prefix_words(mut sql: &str) -> Result<Vec<String>, DriverError> {
    let mut words = Vec::new();
    for _ in 0..4 {
        sql = leading_body(sql)?;
        let end = sql
            .find(|ch: char| !ch.is_ascii_alphabetic() && ch != '_')
            .unwrap_or(sql.len());
        if end == 0 {
            break;
        }
        words.push(sql[..end].to_ascii_uppercase());
        sql = &sql[end..];
    }
    Ok(words)
}

fn executable_comment(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            let quote = bytes[index];
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == quote {
                    index += 1;
                    if bytes.get(index) == Some(&quote) {
                        index += 1;
                    } else {
                        break;
                    }
                } else {
                    index += 1;
                }
            }
        } else if bytes[index] == b'$' {
            if let Some(end) = crate::sql_dump::find_dollar_tag(bytes, index) {
                let tag = &sql[index..end];
                if let Some(close) = sql[end..].find(tag) {
                    index = end + close + tag.len();
                } else {
                    index = bytes.len();
                }
            } else {
                index += 1;
            }
        } else if bytes[index..].starts_with(b"--") {
            index = sql[index..]
                .find('\n')
                .map_or(bytes.len(), |end| index + end + 1);
        } else if bytes[index..].starts_with(b"/*") {
            if bytes[index..].starts_with(b"/*!") || bytes[index..].starts_with(b"/*M!") {
                return true;
            }
            index = sql[index + 2..]
                .find("*/")
                .map_or(bytes.len(), |end| index + end + 4);
        } else {
            index += 1;
        }
    }
    false
}
