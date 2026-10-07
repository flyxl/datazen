//! Dialect-neutral SQL text construction.
//!
//! Every SQL driver has to quote an identifier and render a literal, so these
//! operations belong to the driver contract. Their bodies live here rather than
//! inside the trait so that the SQL text rules can be read and changed in one
//! place, instead of being interleaved with a hundred unrelated signatures.
//!
//! Overriding still works exactly as before: the builders below dispatch through
//! `driver.<method>`, never through these functions directly. Each function here
//! is the default body of the trait method that carries the same name.

use super::DatabaseDriver;
use crate::sql_target::SqlTarget;
use crate::types::{PaginationSyntax, Value};

/// Lowercase hexadecimal encoding used by the default literal formatter.
///
/// It stays private to this module: only the default literal formatting uses it,
/// and a dialect that needs a different binary literal syntax overrides that
/// method instead of borrowing this helper.
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn quote_ident<D: DatabaseDriver + ?Sized>(driver: &D, name: &str) -> String {
    let q = driver.quote_char();
    if q == '`' {
        format!("`{}`", name.replace('`', "``"))
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

pub(crate) fn pagination_syntax<D: DatabaseDriver + ?Sized>(
    driver: &D,
    limit: u64,
    offset: u64,
) -> PaginationSyntax {
    PaginationSyntax {
        clause: if driver.supports_offset() {
            format!("LIMIT {limit} OFFSET {offset}")
        } else {
            format!("LIMIT {limit}")
        },
        requires_order_by: false,
        order_by_fallback: None,
    }
}

pub(crate) fn format_sql_literal(value: &Option<Value>) -> String {
    match value {
        None | Some(Value::Null) => "NULL".to_string(),
        Some(Value::Bool(b)) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Some(Value::Integer(i)) => i.to_string(),
        Some(Value::Float(f)) => f.to_string(),
        Some(Value::String(s)) => {
            let escaped = s.replace('\\', "\\\\");
            format!("'{}'", escaped.replace('\'', "''"))
        }
        Some(Value::Bytes(b)) => {
            // Keep the default dialect conservative and lossless. Drivers
            // with a stricter binary-literal grammar should override this
            // method (PostgreSQL uses bytea hex input; MySQL/SQLite use
            // X'...'). Never turn arbitrary bytes into replacement UTF-8.
            format!("X'{}'", bytes_to_hex(b))
        }
        Some(Value::Timestamp(s)) => {
            let escaped = s.replace('\\', "\\\\");
            format!("'{}'", escaped.replace('\'', "''"))
        }
        Some(Value::Json(j)) => {
            let s = j.to_string();
            let escaped = s.replace('\\', "\\\\");
            format!("'{}'", escaped.replace('\'', "''"))
        }
    }
}

pub(crate) fn build_update_sql<D: DatabaseDriver + ?Sized>(
    driver: &D,
    table: &str,
    set_columns: &[(&str, Option<Value>)],
    pk_columns: &[(&str, Option<Value>)],
) -> String {
    let set_clauses: Vec<String> = set_columns
        .iter()
        .map(|(col, val)| {
            format!(
                "{} = {}",
                driver.quote_ident(col),
                driver.format_sql_literal(val)
            )
        })
        .collect();
    let where_clauses: Vec<String> = pk_columns
        .iter()
        .map(|(col, val)| match val {
            None | Some(Value::Null) => format!("{} IS NULL", driver.quote_ident(col)),
            Some(v) => format!(
                "{} = {}",
                driver.quote_ident(col),
                driver.format_sql_literal(&Some(v.clone()))
            ),
        })
        .collect();
    format!(
        "UPDATE {} SET {} WHERE {}",
        driver.quote_ident(table),
        set_clauses.join(", "),
        where_clauses.join(" AND ")
    )
}

pub(crate) fn build_delete_sql<D: DatabaseDriver + ?Sized>(
    driver: &D,
    table: &str,
    pk_columns: &[(&str, Option<Value>)],
) -> String {
    let where_clauses: Vec<String> = pk_columns
        .iter()
        .map(|(col, val)| match val {
            None | Some(Value::Null) => format!("{} IS NULL", driver.quote_ident(col)),
            Some(v) => format!(
                "{} = {}",
                driver.quote_ident(col),
                driver.format_sql_literal(&Some(v.clone()))
            ),
        })
        .collect();
    format!(
        "DELETE FROM {} WHERE {}",
        driver.quote_ident(table),
        where_clauses.join(" AND ")
    )
}

pub(crate) fn qualified_sql<D: DatabaseDriver + ?Sized>(
    driver: &D,
    sql: &str,
    target: SqlTarget<'_>,
) -> String {
    if !target.is_present() {
        return sql.to_string();
    }
    driver
        .qualify_sql_target(sql, target.database, target.schema)
        .unwrap_or_else(|| sql.to_string())
}

pub(crate) fn split_restore_sql<D: DatabaseDriver + ?Sized>(driver: &D, sql: &str) -> Vec<String> {
    let mut scanner = driver.new_sql_scanner();
    let mut out = scanner.push(sql);
    out.extend(scanner.finish());
    out
}
