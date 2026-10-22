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
use crate::dialect::LegacySqlDialect;
use crate::sql_target::SqlTarget;
use crate::types::{BoundSqlStatement, DriverError, PaginationSyntax, SqlLiteralDialect, Value};

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

// ── The row-write statement builders ─────────────────────────────────
//
// These are the default bodies of the trait methods with the same names.
// They live here because they are SQL text: the rules (which dialect quotes
// how, how many parameters a statement may bind, when a driver may not write
// at all) are one story, and the trait declaration stays a one-liner.
//
// The bodies are unchanged by living here — they are the same statements the
// trait produced before, byte for byte.

/// The legacy inline-value `UPDATE`.
pub(crate) fn build_update_sql<D: DatabaseDriver + ?Sized>(
    driver: &D,
    table: &str,
    set_columns: &[(&str, Option<Value>)],
    pk_columns: &[(&str, Option<Value>)],
) -> String {
    crate::sql_builder::build_update_sql(
        &LegacySqlDialect::new(driver),
        table,
        set_columns,
        pk_columns,
    )
}

/// The legacy inline-value `DELETE`.
pub(crate) fn build_delete_sql<D: DatabaseDriver + ?Sized>(
    driver: &D,
    table: &str,
    pk_columns: &[(&str, Option<Value>)],
) -> String {
    crate::sql_builder::build_delete_sql(&LegacySqlDialect::new(driver), table, pk_columns)
}

/// The parameterized `UPDATE`: values stay out of the SQL text.
pub(crate) fn build_update_statement<D: DatabaseDriver + ?Sized>(
    driver: &D,
    table: &str,
    set_columns: &[(&str, Option<Value>)],
    pk_columns: &[(&str, Option<Value>)],
) -> Result<BoundSqlStatement, DriverError> {
    if !driver.supports_bound_writes() {
        return Err(DriverError::Unsupported(
            "parameterized row writes are not supported".into(),
        ));
    }
    crate::sql_builder::build_update_statement(
        &LegacySqlDialect::new(driver),
        driver.max_bound_parameters(),
        table,
        set_columns,
        pk_columns,
    )
}

/// The parameterized `DELETE`: values stay out of the SQL text.
pub(crate) fn build_delete_statement<D: DatabaseDriver + ?Sized>(
    driver: &D,
    table: &str,
    pk_columns: &[(&str, Option<Value>)],
) -> Result<BoundSqlStatement, DriverError> {
    if !driver.supports_bound_writes() {
        return Err(DriverError::Unsupported(
            "parameterized row writes are not supported".into(),
        ));
    }
    crate::sql_builder::build_delete_statement(
        &LegacySqlDialect::new(driver),
        driver.max_bound_parameters(),
        table,
        pk_columns,
    )
}

/// The parameterized `INSERT`: values stay out of the SQL text.
pub(crate) fn build_insert_statement<D: DatabaseDriver + ?Sized>(
    driver: &D,
    table: &str,
    values: &[(&str, Option<Value>)],
) -> Result<BoundSqlStatement, DriverError> {
    if !driver.supports_bound_writes() {
        return Err(DriverError::Unsupported(
            "parameterized row writes are not supported".into(),
        ));
    }
    crate::sql_builder::build_insert_statement(
        &LegacySqlDialect::new(driver),
        driver.max_bound_parameters(),
        table,
        values,
    )
}

/// The dialect clause that makes a new row's primary key available, or `None`
/// when this dialect cannot return it.
///
/// The host appends `Some(clause)` to the insert and reads the returned row.
/// `Some("")` is the "already inlined" signal: the driver's
/// `build_insert_statement` placed the clause itself (T-SQL
/// `OUTPUT INSERTED.*` cannot follow `VALUES`), so the host must not append
/// anything — not even a space — and must read through the query path.
/// `None` means the host must not guess: it refreshes the page afterwards.
///
/// The default is `None`, which is the pre-existing behaviour: a dialect that
/// cannot backfill the new key says so instead of the host inventing one.
pub(crate) fn insert_returning_clause(_columns: &[&str]) -> Option<String> {
    None
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

    fn parameterized_mock(
        extra: impl FnOnce(&mut MockDriverOptions),
    ) -> std::sync::Arc<MockDriver> {
        let mut options = MockDriverOptions::default();
        options.parameterized_writes = true;
        extra(&mut options);
        MockDriver::new("sqlite", options)
    }

    #[test]
    fn parameterized_insert_distinguishes_null_from_an_unwritten_column() {
        let driver = parameterized_mock(|_| {});
        let insert = driver
            .build_insert_statement(
                "rows",
                &[
                    ("name", Some(Value::String("O'Brien".into()))),
                    ("note", None),
                ],
            )
            .unwrap();

        // `id` is absent: it is Unset (DEFAULT), not NULL.
        assert_eq!(
            insert.sql,
            "INSERT INTO \"rows\" (\"name\", \"note\") VALUES (?1, ?2)"
        );
        assert_eq!(insert.parameters.len(), 2);
        assert!(matches!(&insert.parameters[0], Value::String(value) if value == "O'Brien"));
        assert!(matches!(&insert.parameters[1], Value::Null));
        assert!(!insert.sql.contains("id"));
    }

    #[test]
    fn parameterized_insert_of_only_defaults_uses_default_values() {
        let driver = parameterized_mock(|_| {});
        let insert = driver.build_insert_statement("rows", &[]).unwrap();
        assert_eq!(insert.sql, "INSERT INTO \"rows\" DEFAULT VALUES");
        assert!(insert.parameters.is_empty());
    }

    #[test]
    fn parameterized_insert_refuses_drivers_without_binding_support() {
        let driver = MockDriver::new("sqlite", MockDriverOptions::default());
        let result = driver.build_insert_statement("rows", &[("name", None)]);
        assert!(matches!(result, Err(DriverError::Unsupported(_))));
        // The returning seam defaults to "cannot backfill", so the host takes
        // the degradation path instead of guessing a primary key.
        assert_eq!(driver.insert_returning_clause(&["id"]), None);
    }

    #[test]
    fn reuse_driver_forwards_insert_builders() {
        let options = MockDriverOptions {
            parameterized_writes: true,
            insert_returning_clause: Some("RETURNING \"id\"".into()),
            ..MockDriverOptions::default()
        };
        let inner: std::sync::Arc<dyn DatabaseDriver> = MockDriver::new("postgres", options);
        let wrapper = crate::ReuseDriver::new(inner.clone(), "questdb");
        let columns: &[(&str, Option<Value>)] =
            &[("name", Some(Value::String("a".into()))), ("note", None)];

        let direct = inner.build_insert_statement("rows", columns).unwrap();
        let forwarded = wrapper.build_insert_statement("rows", columns).unwrap();
        assert_eq!(forwarded.sql, direct.sql);
        assert_eq!(forwarded.parameters.len(), direct.parameters.len());
        assert!(forwarded
            .sql
            .starts_with("INSERT INTO \"rows\" (\"name\", \"note\")"));
        assert_eq!(
            wrapper.insert_returning_clause(&["id"]),
            Some("RETURNING \"id\"".into()),
            "a wrapper that stops forwarding hides the inner driver's RETURNING support"
        );
    }
}
