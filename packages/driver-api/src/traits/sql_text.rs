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
use crate::types::{DriverError, PaginationSyntax, SqlLiteralDialect, Value};

/// Lowercase hexadecimal encoding used by the default literal formatter.
///
/// It stays private to this module: only the default literal formatting uses it,
/// and a dialect that needs a different binary literal syntax overrides that
/// method instead of borrowing this helper.
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn utf16le_to_hex(text: &str) -> String {
    let bytes = text
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    bytes_to_hex(&bytes)
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

pub(crate) fn format_sql_literal_for_dialect(
    value: &Option<Value>,
    dialect: SqlLiteralDialect,
) -> Result<String, DriverError> {
    let string_literal = |value: &str| -> Result<String, DriverError> {
        if value.contains('\0')
            && matches!(
                dialect,
                SqlLiteralDialect::DuckDb | SqlLiteralDialect::Postgres
            )
        {
            return Err(DriverError::Unsupported(
                "this server does not accept NUL characters in text literals".into(),
            ));
        }
        Ok(match dialect {
            SqlLiteralDialect::ClickHouse => format!("unhex('{}')", bytes_to_hex(value.as_bytes())),
            SqlLiteralDialect::MySql => {
                format!(
                    "CONVERT(X'{}' USING utf8mb4)",
                    bytes_to_hex(value.as_bytes())
                )
            }
            SqlLiteralDialect::DuckDb | SqlLiteralDialect::Postgres => {
                let mut tag = "datazen".to_string();
                let mut delimiter = format!("${tag}$");
                let mut suffix = 0usize;
                while value.contains(&delimiter) {
                    suffix += 1;
                    tag = format!("datazen_{suffix}");
                    delimiter = format!("${tag}$");
                }
                format!("{delimiter}{value}{delimiter}")
            }
            SqlLiteralDialect::SqlServer => {
                format!("CONVERT(nvarchar(max), 0x{})", utf16le_to_hex(value))
            }
            SqlLiteralDialect::Sqlite => {
                format!("CAST(X'{}' AS TEXT)", bytes_to_hex(value.as_bytes()))
            }
        })
    };

    let bytes_literal = |bytes: &[u8]| match dialect {
        SqlLiteralDialect::ClickHouse => format!("unhex('{}')", bytes_to_hex(bytes)),
        SqlLiteralDialect::DuckDb => format!("from_hex('{}')", bytes_to_hex(bytes)),
        SqlLiteralDialect::MySql | SqlLiteralDialect::Sqlite => {
            format!("X'{}'", bytes_to_hex(bytes))
        }
        SqlLiteralDialect::Postgres => {
            format!("decode('{}', 'hex')", bytes_to_hex(bytes))
        }
        SqlLiteralDialect::SqlServer => format!("0x{}", bytes_to_hex(bytes)),
    };

    match value {
        None | Some(Value::Null) => Ok("NULL".to_string()),
        Some(Value::Bool(b)) => Ok(match (dialect, b) {
            (
                SqlLiteralDialect::MySql | SqlLiteralDialect::Sqlite | SqlLiteralDialect::SqlServer,
                true,
            ) => "1",
            (
                SqlLiteralDialect::MySql | SqlLiteralDialect::Sqlite | SqlLiteralDialect::SqlServer,
                false,
            ) => "0",
            (_, true) => "TRUE",
            (_, false) => "FALSE",
        }
        .to_string()),
        Some(Value::Integer(i)) => Ok(i.to_string()),
        Some(Value::Float(f)) if f.is_finite() => Ok(f.to_string()),
        Some(Value::Float(_)) => Err(DriverError::Unsupported(
            "non-finite floating-point values cannot be rendered as SQL literals".into(),
        )),
        Some(Value::String(s) | Value::Timestamp(s)) => string_literal(s),
        Some(Value::Bytes(bytes)) => Ok(bytes_literal(bytes)),
        Some(Value::Json(json)) => string_literal(&json.to_string()),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_driver::{MockDriver, MockDriverOptions};

    #[test]
    fn literal_formatters_do_not_reinterpret_backslashes() {
        let text = "quote' slash\\ trailing\\\n雪 $datazen$";
        let value = Some(Value::String(text.to_string()));
        let hex = bytes_to_hex(text.as_bytes());

        assert_eq!(
            format_sql_literal_for_dialect(&value, SqlLiteralDialect::MySql).unwrap(),
            format!("CONVERT(X'{hex}' USING utf8mb4)")
        );
        assert_eq!(
            format_sql_literal_for_dialect(&value, SqlLiteralDialect::ClickHouse).unwrap(),
            format!("unhex('{hex}')")
        );
        assert_eq!(
            format_sql_literal_for_dialect(&value, SqlLiteralDialect::Sqlite).unwrap(),
            format!("CAST(X'{hex}' AS TEXT)")
        );
        assert_eq!(
            format_sql_literal_for_dialect(&value, SqlLiteralDialect::Postgres).unwrap(),
            format!("$datazen_1${text}$datazen_1$")
        );
        assert_eq!(
            format_sql_literal_for_dialect(&value, SqlLiteralDialect::DuckDb).unwrap(),
            format!("$datazen_1${text}$datazen_1$")
        );
        assert_eq!(
            format_sql_literal_for_dialect(&value, SqlLiteralDialect::SqlServer).unwrap(),
            format!("CONVERT(nvarchar(max), 0x{})", utf16le_to_hex(text))
        );
    }

    #[test]
    fn postgres_and_duckdb_text_literals_reject_nul_bytes() {
        let value = Some(Value::String("before\0after".to_string()));
        for dialect in [SqlLiteralDialect::Postgres, SqlLiteralDialect::DuckDb] {
            assert!(format_sql_literal_for_dialect(&value, dialect).is_err());
        }
        assert!(format_sql_literal_for_dialect(&value, SqlLiteralDialect::Sqlite).is_ok());
    }

    #[test]
    fn parameterized_update_and_delete_keep_values_out_of_sql() {
        let mut options = MockDriverOptions::default();
        options.parameterized_writes = true;
        let driver = MockDriver::new("sqlite", options);
        let update = driver
            .build_update_statement(
                "rows",
                &[
                    ("text", Some(Value::String("x'\\y".to_string()))),
                    ("nullable", None),
                ],
                &[
                    ("id", Some(Value::Integer(7))),
                    ("tenant", Some(Value::Null)),
                ],
            )
            .unwrap();

        assert_eq!(
            update.sql,
            "UPDATE \"rows\" SET \"text\" = ?1, \"nullable\" = ?2 WHERE \"id\" = ?3 AND \"tenant\" IS NULL"
        );
        assert_eq!(update.parameters.len(), 3);
        assert!(matches!(&update.parameters[0], Value::String(value) if value == "x'\\y"));
        assert!(matches!(update.parameters[1], Value::Null));
        assert!(matches!(update.parameters[2], Value::Integer(7)));

        let delete = driver
            .build_delete_statement(
                "rows",
                &[
                    ("id", Some(Value::Integer(7))),
                    ("tenant", Some(Value::Null)),
                ],
            )
            .unwrap();
        assert_eq!(
            delete.sql,
            "DELETE FROM \"rows\" WHERE \"id\" = ?1 AND \"tenant\" IS NULL"
        );
        assert!(matches!(delete.parameters.as_slice(), [Value::Integer(7)]));
    }

    #[test]
    fn parameterized_statement_refuses_drivers_without_binding_support() {
        let driver = MockDriver::new("sqlite", MockDriverOptions::default());
        let result = driver.build_delete_statement("rows", &[("id", Some(Value::Integer(1)))]);
        assert!(matches!(result, Err(DriverError::Unsupported(_))));
    }

    #[test]
    fn legacy_generic_literal_formatter_does_not_double_backslashes() {
        assert_eq!(
            crate::sql_builder::format_sql_literal_ansi(&Some(Value::String("a\\b".into()))),
            "'a\\b'"
        );
    }
}
