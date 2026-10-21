//! Shared SQL statement construction over [`SqlDialect`].
//!
//! The builders below depend on the narrow text contract and on an explicit
//! bound-parameter budget — never on a driver, a connection or a resource
//! pool. A caller that happens to hold a driver wraps it with
//! [`LegacySqlDialect`](crate::dialect::LegacySqlDialect) at its own boundary;
//! a caller that only has dialect rules (a preview, a migration planner, a
//! test) passes a dialect implementation directly. Pure SQL generation must not
//! reach for a placeholder through the connection layer, which is exactly what
//! this module prevents by construction.

use crate::dialect::SqlDialect;
use crate::types::{BoundSqlStatement, DriverError, Value};

/// Historical, infallible ANSI-style literal rendering.
///
/// Only the deprecated `build_*_sql` string builders use this; product write
/// paths must go through [`SqlDialect::try_format_sql_literal`], which refuses
/// dialects without an injective grammar. It stays as the default body of
/// [`SqlDialect::legacy_format_sql_literal`] so a driver that never overrode
/// the old method keeps producing byte-identical SQL during migration.
pub(crate) fn format_sql_literal_ansi(value: &Option<Value>) -> String {
    match value {
        None | Some(Value::Null) => "NULL".to_string(),
        Some(Value::Bool(true)) => "TRUE".to_string(),
        Some(Value::Bool(false)) => "FALSE".to_string(),
        Some(Value::Integer(i)) => i.to_string(),
        Some(Value::Float(f)) => f.to_string(),
        Some(Value::String(s) | Value::Timestamp(s)) => {
            format!("'{}'", s.replace('\'', "''"))
        }
        Some(Value::Bytes(b)) => {
            let hex: String = b.iter().map(|byte| format!("{byte:02x}")).collect();
            format!("X'{hex}'")
        }
        Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
    }
}

/// Build the deprecated inline-value `UPDATE` string.
///
/// Values are interpolated through [`SqlDialect::legacy_format_sql_literal`];
/// product code must use [`build_update_statement`] instead so values stay
/// bound and out of the SQL text.
pub fn build_update_sql(
    dialect: &dyn SqlDialect,
    table: &str,
    set_columns: &[(&str, Option<Value>)],
    pk_columns: &[(&str, Option<Value>)],
) -> String {
    let set_clauses: Vec<String> = set_columns
        .iter()
        .map(|(column, value)| {
            format!(
                "{} = {}",
                dialect.quote_ident(column),
                dialect.legacy_format_sql_literal(value)
            )
        })
        .collect();
    let where_clauses: Vec<String> = pk_columns
        .iter()
        .map(|(column, value)| match value {
            None | Some(Value::Null) => format!("{} IS NULL", dialect.quote_ident(column)),
            Some(value) => format!(
                "{} = {}",
                dialect.quote_ident(column),
                dialect.legacy_format_sql_literal(&Some(value.clone()))
            ),
        })
        .collect();
    format!(
        "UPDATE {} SET {} WHERE {}",
        dialect.quote_ident(table),
        set_clauses.join(", "),
        where_clauses.join(" AND ")
    )
}

/// Build the deprecated inline-value `DELETE` string.
pub fn build_delete_sql(
    dialect: &dyn SqlDialect,
    table: &str,
    pk_columns: &[(&str, Option<Value>)],
) -> String {
    let where_clauses: Vec<String> = pk_columns
        .iter()
        .map(|(column, value)| match value {
            None | Some(Value::Null) => format!("{} IS NULL", dialect.quote_ident(column)),
            Some(value) => format!(
                "{} = {}",
                dialect.quote_ident(column),
                dialect.legacy_format_sql_literal(&Some(value.clone()))
            ),
        })
        .collect();
    format!(
        "DELETE FROM {} WHERE {}",
        dialect.quote_ident(table),
        where_clauses.join(" AND ")
    )
}

/// Build an `UPDATE` whose values stay bound.
///
/// `max_parameters` is supplied by the caller — it is the binding capability
/// the caller has already established for this statement, not something the
/// builder discovers from a driver. Exceeding it fails before any SQL is
/// returned, so a caller cannot submit a statement the server would reject
/// halfway through the batch.
pub fn build_update_statement(
    dialect: &dyn SqlDialect,
    max_parameters: usize,
    table: &str,
    set_columns: &[(&str, Option<Value>)],
    pk_columns: &[(&str, Option<Value>)],
) -> Result<BoundSqlStatement, DriverError> {
    let mut parameters = Vec::new();
    let mut bind = |value: &Option<Value>| -> Result<String, DriverError> {
        bind_parameter(dialect, max_parameters, &mut parameters, value)
    };

    let mut set_clauses = Vec::with_capacity(set_columns.len());
    for (column, value) in set_columns {
        set_clauses.push(format!(
            "{} = {}",
            dialect.quote_ident(column),
            bind(value)?
        ));
    }
    let mut where_clauses = Vec::with_capacity(pk_columns.len());
    for (column, value) in pk_columns {
        let quoted = dialect.quote_ident(column);
        if value
            .as_ref()
            .map_or(true, |value| matches!(value, Value::Null))
        {
            where_clauses.push(format!("{quoted} IS NULL"));
        } else {
            where_clauses.push(format!("{quoted} = {}", bind(value)?));
        }
    }

    Ok(BoundSqlStatement {
        sql: format!(
            "UPDATE {} SET {} WHERE {}",
            dialect.quote_ident(table),
            set_clauses.join(", "),
            where_clauses.join(" AND ")
        ),
        parameters,
    })
}

/// Build a `DELETE` whose values stay bound. Same budget contract as
/// [`build_update_statement`].
pub fn build_delete_statement(
    dialect: &dyn SqlDialect,
    max_parameters: usize,
    table: &str,
    pk_columns: &[(&str, Option<Value>)],
) -> Result<BoundSqlStatement, DriverError> {
    let mut parameters = Vec::new();
    let mut where_clauses = Vec::with_capacity(pk_columns.len());
    for (column, value) in pk_columns {
        let quoted = dialect.quote_ident(column);
        if value
            .as_ref()
            .map_or(true, |value| matches!(value, Value::Null))
        {
            where_clauses.push(format!("{quoted} IS NULL"));
        } else {
            let placeholder = bind_parameter(dialect, max_parameters, &mut parameters, value)?;
            where_clauses.push(format!("{quoted} = {placeholder}"));
        }
    }

    Ok(BoundSqlStatement {
        sql: format!(
            "DELETE FROM {} WHERE {}",
            dialect.quote_ident(table),
            where_clauses.join(" AND ")
        ),
        parameters,
    })
}

fn bind_parameter(
    dialect: &dyn SqlDialect,
    max_parameters: usize,
    parameters: &mut Vec<Value>,
    value: &Option<Value>,
) -> Result<String, DriverError> {
    let index = parameters.len() + 1;
    if index > max_parameters {
        return Err(DriverError::Unsupported(format!(
            "statement requires more than {max_parameters} bound parameters"
        )));
    }
    let placeholder = dialect.parameter_placeholder(index, None)?;
    parameters.push(value.clone().unwrap_or(Value::Null));
    Ok(placeholder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::BoundParameterLimit;
    use crate::types::PaginationSyntax;

    /// A dialect with no driver behind it: the whole point of the narrow
    /// contract is that builders can be exercised without one.
    struct TestDialect {
        max_parameters: usize,
        placeholder_error: bool,
    }

    impl SqlDialect for TestDialect {
        fn quote_ident(&self, name: &str) -> String {
            format!("`{}`", name.replace('`', "``"))
        }

        fn pagination_syntax(&self, _limit: u64, _offset: u64) -> PaginationSyntax {
            PaginationSyntax {
                clause: String::new(),
                requires_order_by: false,
                order_by_fallback: None,
            }
        }

        fn try_format_sql_literal(&self, value: &Option<Value>) -> Result<String, DriverError> {
            Ok(format_sql_literal_ansi(value))
        }

        fn parameter_placeholder(
            &self,
            index: usize,
            _data_type: Option<&str>,
        ) -> Result<String, DriverError> {
            if self.placeholder_error {
                return Err(DriverError::Unsupported("no binding".into()));
            }
            Ok(format!("?{index}"))
        }

        fn max_bound_parameters(&self) -> BoundParameterLimit {
            BoundParameterLimit::Dialect(self.max_parameters)
        }
    }

    fn dialect() -> TestDialect {
        TestDialect {
            max_parameters: 60_000,
            placeholder_error: false,
        }
    }

    #[test]
    fn update_and_delete_sql_use_the_dialect_quoting_and_literals() {
        let dialect = dialect();
        let update = build_update_sql(
            &dialect,
            "users",
            &[("name", Some(Value::String("O'Brien".into())))],
            &[
                ("id", Some(Value::Integer(7))),
                ("tenant", Some(Value::Null)),
            ],
        );
        assert_eq!(
            update,
            "UPDATE `users` SET `name` = 'O''Brien' WHERE `id` = 7 AND `tenant` IS NULL"
        );

        let delete = build_delete_sql(&dialect, "users", &[("id", Some(Value::Integer(7)))]);
        assert_eq!(delete, "DELETE FROM `users` WHERE `id` = 7");
    }

    #[test]
    fn bound_statements_keep_values_out_of_sql_and_skip_null_predicates() {
        let dialect = dialect();
        let update = build_update_statement(
            &dialect,
            10,
            "rows",
            &[
                ("text", Some(Value::String("x'\\y".into()))),
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
            "UPDATE `rows` SET `text` = ?1, `nullable` = ?2 WHERE `id` = ?3 AND `tenant` IS NULL"
        );
        assert_eq!(update.parameters.len(), 3);

        let delete = build_delete_statement(
            &dialect,
            10,
            "rows",
            &[("id", Some(Value::Integer(7))), ("tenant", None)],
        )
        .unwrap();
        assert_eq!(
            delete.sql,
            "DELETE FROM `rows` WHERE `id` = ?1 AND `tenant` IS NULL"
        );
        assert!(matches!(delete.parameters.as_slice(), [Value::Integer(7)]));
    }

    #[test]
    fn exceeding_the_explicit_parameter_budget_fails_before_sql_is_returned() {
        let error = build_delete_statement(
            &dialect(),
            1,
            "rows",
            &[
                ("id", Some(Value::Integer(1))),
                ("rev", Some(Value::Integer(2))),
            ],
        )
        .unwrap_err();
        assert!(matches!(error, DriverError::Unsupported(_)));
    }

    #[test]
    fn an_unsupported_placeholder_grammar_fails_closed() {
        let dialect = TestDialect {
            max_parameters: 10,
            placeholder_error: true,
        };
        let error =
            build_update_statement(&dialect, 10, "rows", &[("a", Some(Value::Integer(1)))], &[])
                .unwrap_err();
        assert!(matches!(error, DriverError::Unsupported(_)));
    }
}
