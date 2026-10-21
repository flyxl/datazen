//! The probe statement's **shape**: it must be a valid statement, it must
//! ask for one row more than the cap, and it must not order the rows.
//!
//! A driver that does not parse SQL (the shared mock does not) is blind to
//! all of this — a statement without its `WHERE` clause reads fine to such a
//! driver and fails on every real database. These cases parse the statement
//! with `sqlparser` instead, so the SQL text itself is under test.

use datazen_driver_api::{
    BoundParameterLimit, DriverError, LegacySqlDialect, PaginationSyntax, SqlDialect, Value,
};
use sqlparser::ast::{LimitClause, Statement};
use sqlparser::dialect::{PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;

use super::*;

/// A dialect whose pagination clause is only legal after an ordering
/// (T-SQL's `OFFSET … FETCH NEXT …`).
struct OrderedPagingDialect;

impl SqlDialect for OrderedPagingDialect {
    fn quote_ident(&self, name: &str) -> String {
        format!("\"{name}\"")
    }

    fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax {
        PaginationSyntax {
            clause: format!("OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY"),
            requires_order_by: true,
            order_by_fallback: Some("(SELECT NULL)"),
        }
    }

    fn try_format_sql_literal(&self, value: &Option<Value>) -> Result<String, DriverError> {
        Ok(match value {
            Some(Value::Integer(number)) => number.to_string(),
            Some(Value::String(text)) => format!("'{text}'"),
            _ => "NULL".to_string(),
        })
    }

    fn parameter_placeholder(
        &self,
        index: usize,
        _data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        Ok(format!("${index}"))
    }

    fn max_bound_parameters(&self) -> BoundParameterLimit {
        BoundParameterLimit::Dialect(999)
    }
}

/// The statement the probe would send, for one target column.
fn statement_for(dialect: &dyn SqlDialect, limit: u32) -> String {
    let statement = build_probe_statement(
        dialect,
        "parents",
        &[column("id", "integer", true)],
        &["id".to_string()],
        &[Value::Integer(7)],
        limit,
    )
    .expect("the probe statement is expressible");
    statement.sql().to_string()
}

#[test]
fn the_probe_statement_is_a_valid_statement() {
    // The regression this guards: a statement that appends its conditions
    // without a `WHERE` clause parses fine for a driver that does not parse
    // SQL, and fails on every real database.
    let driver = MockDriver::new("postgres", MockDriverOptions::default());
    let dialect = LegacySqlDialect::new(driver.as_ref());
    let sql = statement_for(&dialect, 50);

    assert!(
        Parser::parse_sql(&PostgreSqlDialect {}, &sql).is_ok(),
        "the probe statement must be valid SQL: {sql}"
    );
    assert!(
        sql.contains(" WHERE "),
        "the conditions need a WHERE clause: {sql}"
    );
}

#[test]
fn the_probe_statement_asks_for_one_row_more_than_the_cap() {
    // The extra row is how the resolver learns that the candidate list was
    // cut off; without it a full page is indistinguishable from a truncated
    // one.
    let driver = MockDriver::new("postgres", MockDriverOptions::default());
    let dialect = LegacySqlDialect::new(driver.as_ref());
    let sql = statement_for(&dialect, 50);

    let statements = Parser::parse_sql(&PostgreSqlDialect {}, &sql).expect("the statement parses");
    let query = match &statements[0] {
        Statement::Query(query) => query,
        other => panic!("expected a query, got {other:?}"),
    };
    let limit = match query.limit_clause.as_ref() {
        Some(LimitClause::LimitOffset { limit, .. }) => {
            limit.as_ref().map(|expression| expression.to_string())
        }
        other => panic!("expected a LIMIT clause, got {other:?}"),
    };

    assert_eq!(
        limit.as_deref(),
        Some("51"),
        "the probe asks for the cap plus one row: {sql}"
    );
}

#[test]
fn the_probe_statement_does_not_order_the_rows() {
    // The command has no ordering parameter (07 U-8): an ordering of its own
    // would silently reorder the target rows, usually by the very key that
    // was looked up.
    let driver = MockDriver::new("postgres", MockDriverOptions::default());
    let dialect = LegacySqlDialect::new(driver.as_ref());
    let sql = statement_for(&dialect, 50);

    let statements = Parser::parse_sql(&PostgreSqlDialect {}, &sql).expect("the statement parses");
    let query = match &statements[0] {
        Statement::Query(query) => query,
        other => panic!("expected a query, got {other:?}"),
    };

    assert!(
        query.order_by.is_none(),
        "the probe must not order the rows: {sql}"
    );
    assert!(
        !sql.contains("ORDER BY"),
        "the probe must not order the rows: {sql}"
    );
}

#[test]
fn the_probe_statement_orders_the_rows_only_when_the_dialect_requires_it() {
    // T-SQL's pagination clause is illegal without an ordering, so the
    // dialect's own fallback expression is used — and only there.
    let sql = statement_for(&OrderedPagingDialect, 50);

    assert!(
        sql.contains("ORDER BY (SELECT NULL)"),
        "a dialect that requires an ordering gets the dialect's own: {sql}"
    );
    assert!(
        sql.contains("OFFSET 0 ROWS FETCH NEXT 51 ROWS ONLY"),
        "the dialect's pagination clause is used as it is: {sql}"
    );
}

#[test]
fn the_bound_probe_statement_is_a_valid_statement() {
    // The bound form differs from the inline one only in the placeholder,
    // but it is the form every SQL driver takes — so it has to parse too.
    let driver = MockDriver::new(
        "postgres",
        MockDriverOptions {
            parameterized_writes: true,
            ..Default::default()
        },
    );
    let dialect = LegacySqlDialect::new(driver.as_ref());
    let sql = statement_for(&dialect, 50);

    assert!(
        sql.contains(" = ?1"),
        "the bound statement carries a placeholder: {sql}"
    );
    assert!(
        Parser::parse_sql(&SQLiteDialect {}, &sql).is_ok(),
        "the bound probe statement must be valid SQL: {sql}"
    );
}
