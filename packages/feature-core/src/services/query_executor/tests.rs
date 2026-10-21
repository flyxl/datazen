//! Tests for the query executor.
//!
//! The whole block lives out of line so the production module stays readable
//! and so the filter tests sit next to the filter module they cover.

use super::*;
use datazen_driver_api::{BoundParameterLimit, PaginationSyntax};

#[derive(Clone, Copy)]
enum TestPaging {
    LimitOffset,
    LimitOnly,
    Tsql,
}

/// A dialect that stands on its own: no driver, no connection, which is the
/// point of the narrowed contract.
struct TestDialect {
    paging: TestPaging,
}

const DEFAULT: TestDialect = TestDialect {
    paging: TestPaging::LimitOffset,
};
const LIMIT_ONLY: TestDialect = TestDialect {
    paging: TestPaging::LimitOnly,
};
const TSQL: TestDialect = TestDialect {
    paging: TestPaging::Tsql,
};

impl SqlDialect for TestDialect {
    fn quote_ident(&self, name: &str) -> String {
        format!("\"{name}\"")
    }

    fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax {
        match self.paging {
            TestPaging::LimitOffset => PaginationSyntax {
                clause: format!("LIMIT {limit} OFFSET {offset}"),
                requires_order_by: false,
                order_by_fallback: None,
            },
            TestPaging::LimitOnly => PaginationSyntax {
                clause: format!("LIMIT {limit}"),
                requires_order_by: false,
                order_by_fallback: None,
            },
            TestPaging::Tsql => PaginationSyntax {
                clause: format!("OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY"),
                requires_order_by: true,
                order_by_fallback: Some("(SELECT NULL)"),
            },
        }
    }

    fn try_format_sql_literal(&self, value: &Option<Value>) -> Result<String, DriverError> {
        Ok(match value {
            None | Some(Value::Null) => "NULL".to_string(),
            Some(Value::Bool(b)) => (if *b { "TRUE" } else { "FALSE" }).to_string(),
            Some(Value::Integer(i)) => i.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s) | Value::Timestamp(s)) => {
                format!("'{}'", s.replace('\'', "''"))
            }
            Some(Value::Bytes(_)) => "NULL".to_string(),
            Some(Value::Json(v)) => format!("'{}'", v.to_string().replace('\'', "''")),
        })
    }

    fn parameter_placeholder(
        &self,
        _index: usize,
        _data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        Err(DriverError::Unsupported(
            "test dialect has no bound placeholders".into(),
        ))
    }

    fn max_bound_parameters(&self) -> BoundParameterLimit {
        BoundParameterLimit::Dialect(999)
    }
}

fn make_column(name: &str, is_pk: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.to_string(),
        data_type: "text".to_string(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: is_pk,
        is_auto_increment: false,
    }
}

/// Regression for the SQL Server `LIMIT` failure: the host must emit the
/// dialect's clause verbatim and add an `ORDER BY` when the read is unordered
/// (`SELECT *` with no columns), because T-SQL `OFFSET … FETCH` requires one.
#[test]
fn tsql_pagination_emits_offset_fetch_and_never_limit() {
    let sql =
        QueryExecutor::build_select_sql("events", &[], None, None, &TSQL, 25, 75, None).unwrap();
    assert!(
        sql.contains("OFFSET 75 ROWS FETCH NEXT 25 ROWS ONLY"),
        "expected T-SQL paging clause, got: {sql}"
    );
    assert!(
        sql.contains("ORDER BY (SELECT NULL)"),
        "unordered read must get the dialect's ORDER BY fallback, got: {sql}"
    );
    assert!(!sql.contains("LIMIT"), "T-SQL has no LIMIT; got: {sql}");
}

/// A table read that already orders by PK must not receive a second ORDER BY.
#[test]
fn tsql_pagination_keeps_pk_order_without_fallback() {
    let columns = vec![make_column("id", true)];
    let sql = QueryExecutor::build_select_sql("orders", &columns, None, None, &TSQL, 50, 0, None)
        .unwrap();
    assert!(sql.contains("ORDER BY \"id\" ASC"), "got: {sql}");
    assert!(!sql.contains("(SELECT NULL)"), "got: {sql}");
    assert!(
        sql.contains("OFFSET 0 ROWS FETCH NEXT 50 ROWS ONLY"),
        "got: {sql}"
    );
    assert!(!sql.contains("LIMIT"), "got: {sql}");
}

#[test]
fn no_explicit_sort_uses_primary_key_order() {
    let columns = vec![
        make_column("id", true),
        make_column("name", false),
        make_column("email", false),
    ];
    let sql =
        QueryExecutor::build_select_sql("orders", &columns, None, None, &DEFAULT, 50, 0, None)
            .unwrap();
    assert!(
        sql.contains("ORDER BY \"id\" ASC"),
        "Expected default ORDER BY primary key, got: {sql}"
    );
}

#[test]
fn no_explicit_sort_composite_pk_uses_all_pk_columns() {
    let columns = vec![
        make_column("order_id", true),
        make_column("product_id", true),
        make_column("quantity", false),
    ];
    let sql =
        QueryExecutor::build_select_sql("order_items", &columns, None, None, &DEFAULT, 50, 0, None)
            .unwrap();
    assert!(
        sql.contains("ORDER BY \"order_id\" ASC, \"product_id\" ASC"),
        "Expected composite PK ordering, got: {sql}"
    );
}

#[test]
fn explicit_sort_overrides_default_pk_order() {
    let columns = vec![make_column("id", true), make_column("name", false)];
    let order = OrderBy {
        column: "name".to_string(),
        descending: true,
    };
    let sql = QueryExecutor::build_select_sql(
        "users",
        &columns,
        None,
        Some(order),
        &DEFAULT,
        50,
        0,
        None,
    )
    .unwrap();
    assert!(
        sql.contains("ORDER BY \"name\" DESC"),
        "Expected explicit ORDER BY, got: {sql}"
    );
    assert!(
        !sql.contains("\"id\" ASC"),
        "Should not contain default PK ordering when explicit sort is given, got: {sql}"
    );
}

#[test]
fn no_pk_no_explicit_sort_still_has_order_by_first_column() {
    let columns = vec![make_column("col_a", false), make_column("col_b", false)];
    let sql =
        QueryExecutor::build_select_sql("no_pk_table", &columns, None, None, &DEFAULT, 50, 0, None)
            .unwrap();
    assert!(
        sql.contains("ORDER BY \"col_a\" ASC"),
        "Expected fallback ORDER BY first column, got: {sql}"
    );
}

#[test]
fn filter_literals_use_dialect_formatter() {
    let columns = vec![make_column("flag", false)];
    let filters = vec![FilterCondition {
        column: "flag".into(),
        operator: FilterOperator::Eq,
        value: Value::Bool(true),
        connector: None,
    }];
    let sql =
        QueryExecutor::build_select_sql("t", &columns, Some(filters), None, &DEFAULT, 10, 0, None)
            .unwrap();
    assert!(
        sql.contains("WHERE \"flag\" = TRUE"),
        "Expected dialect-style TRUE literal, got: {sql}"
    );
}

#[test]
fn build_count_sql_with_eq_filter() {
    let columns = vec![make_column("status", false)];
    let filters = vec![FilterCondition {
        column: "status".into(),
        operator: FilterOperator::Eq,
        value: Value::String("active".into()),
        connector: None,
    }];
    let sql =
        QueryExecutor::build_count_sql("users", &columns, &Some(filters), &DEFAULT, None).unwrap();
    assert!(sql.contains("WHERE \"status\" = 'active'"));
}

#[test]
fn build_select_sql_joins_filters_with_or() {
    let columns = vec![make_column("status", false), make_column("role", false)];
    let filters = vec![
        FilterCondition {
            column: "status".into(),
            operator: FilterOperator::Eq,
            value: Value::String("active".into()),
            connector: None,
        },
        FilterCondition {
            column: "role".into(),
            operator: FilterOperator::Eq,
            value: Value::String("admin".into()),
            connector: None,
        },
    ];
    let sql = QueryExecutor::build_select_sql(
        "users",
        &columns,
        Some(filters),
        None,
        &DEFAULT,
        10,
        0,
        Some("or"),
    )
    .unwrap();
    assert!(
        sql.contains("WHERE \"status\" = 'active' OR \"role\" = 'admin'"),
        "got: {sql}"
    );
}

#[test]
fn build_count_sql_joins_filters_with_or() {
    let columns = vec![make_column("status", false), make_column("role", false)];
    let filters = vec![
        FilterCondition {
            column: "status".into(),
            operator: FilterOperator::Eq,
            value: Value::String("active".into()),
            connector: None,
        },
        FilterCondition {
            column: "role".into(),
            operator: FilterOperator::Eq,
            value: Value::String("admin".into()),
            connector: None,
        },
    ];
    let sql =
        QueryExecutor::build_count_sql("users", &columns, &Some(filters), &DEFAULT, Some("OR"))
            .unwrap();
    assert!(
        sql.contains("WHERE \"status\" = 'active' OR \"role\" = 'admin'"),
        "got: {sql}"
    );
}

#[test]
fn filter_join_defaults_to_and() {
    assert_eq!(filter_sql::filter_join(None), " AND ");
    assert_eq!(filter_sql::filter_join(Some("and")), " AND ");
    assert_eq!(filter_sql::filter_join(Some("or")), " OR ");
}

#[test]
fn build_select_sql_without_offset_when_unsupported() {
    let columns = vec![make_column("id", true)];
    let sql =
        QueryExecutor::build_select_sql("users", &columns, None, None, &LIMIT_ONLY, 25, 0, None)
            .unwrap();
    assert!(sql.contains("LIMIT 25"));
    assert!(!sql.contains("OFFSET"));
}

#[test]
fn format_condition_in_and_is_null() {
    let in_filter = FilterCondition {
        column: "id".into(),
        operator: FilterOperator::In,
        value: Value::Json(serde_json::json!([1, 2, 3])),
        connector: None,
    };
    let null_filter = FilterCondition {
        column: "deleted_at".into(),
        operator: FilterOperator::IsNull,
        value: Value::Null,
        connector: None,
    };
    assert_eq!(
        filter_sql::format_condition(&in_filter, &DEFAULT).unwrap(),
        "\"id\" IN ('1', '2', '3')"
    );
    assert_eq!(
        filter_sql::format_condition(&null_filter, &DEFAULT).unwrap(),
        "\"deleted_at\" IS NULL"
    );
}

#[test]
fn incomplete_eq_filter_is_rejected() {
    let columns = vec![make_column("id", true), make_column("name", false)];
    let filters = vec![FilterCondition {
        column: "id".into(),
        operator: FilterOperator::Eq,
        value: Value::String(String::new()),
        connector: None,
    }];
    let rejection = QueryExecutor::build_select_sql(
        "users",
        &columns,
        Some(filters),
        None,
        &DEFAULT,
        50,
        0,
        None,
    )
    .expect_err("an incomplete filter must be refused, not skipped");
    assert!(
        rejection
            .contract_message()
            .starts_with("grid.filter.incomplete: "),
        "got: {}",
        rejection.contract_message()
    );
}

#[test]
fn one_incomplete_filter_rejects_the_whole_query() {
    // The old behaviour skipped the incomplete condition and ran the query
    // with the remaining one, which returned more rows than the user asked
    // for. The list is now refused as a whole, and the message names the
    // condition that has to be fixed.
    let columns = vec![make_column("id", true), make_column("name", false)];
    let filters = vec![
        FilterCondition {
            column: "id".into(),
            operator: FilterOperator::Eq,
            value: Value::String(String::new()),
            connector: None,
        },
        FilterCondition {
            column: "name".into(),
            operator: FilterOperator::Eq,
            value: Value::String("alice".into()),
            connector: None,
        },
    ];
    let rejection = QueryExecutor::build_select_sql(
        "users",
        &columns,
        Some(filters),
        None,
        &DEFAULT,
        50,
        0,
        Some("and"),
    )
    .expect_err("the incomplete condition must not be silently dropped");
    let message = rejection.contract_message();
    assert!(
        message.starts_with("grid.filter.incomplete: "),
        "got: {message}"
    );
    assert!(message.contains("\"id\""), "got: {message}");
}

#[test]
fn count_and_select_share_one_where() {
    let columns = vec![make_column("id", true), make_column("name", false)];
    let filters = vec![
        FilterCondition {
            column: "id".into(),
            operator: FilterOperator::Eq,
            value: Value::Integer(1),
            connector: None,
        },
        FilterCondition {
            column: "name".into(),
            operator: FilterOperator::Eq,
            value: Value::String("alice".into()),
            connector: Some(FilterConnector::Or),
        },
    ];
    let select = QueryExecutor::build_select_sql(
        "users",
        &columns,
        Some(filters.clone()),
        None,
        &DEFAULT,
        50,
        0,
        Some("and"),
    )
    .unwrap();
    let count =
        QueryExecutor::build_count_sql("users", &columns, &Some(filters), &DEFAULT, Some("and"))
            .unwrap();
    let where_of = |sql: &str| {
        sql.split(" WHERE ")
            .nth(1)
            .expect("both statements carry a WHERE")
            .split(" ORDER BY ")
            .next()
            .expect("the WHERE clause is not empty")
            .to_string()
    };
    assert_eq!(where_of(&select), where_of(&count));
}

#[test]
fn mixed_connectors_are_parenthesised_in_both_statements() {
    let columns = vec![make_column("id", true), make_column("name", false)];
    let filters = vec![
        FilterCondition {
            column: "id".into(),
            operator: FilterOperator::Eq,
            value: Value::Integer(1),
            connector: None,
        },
        FilterCondition {
            column: "name".into(),
            operator: FilterOperator::Eq,
            value: Value::String("alice".into()),
            connector: Some(FilterConnector::Or),
        },
    ];
    let select = QueryExecutor::build_select_sql(
        "users",
        &columns,
        Some(filters.clone()),
        None,
        &DEFAULT,
        50,
        0,
        Some("and"),
    )
    .unwrap();
    // The OR is the outermost operator here, so it needs no parentheses: the
    // statement says exactly what the rows said.
    assert!(
        select.contains("WHERE \"id\" = 1 OR \"name\" = 'alice'"),
        "got: {select}"
    );

    // Adding an AND after the OR group is what forces the parentheses: SQL
    // binds AND first, so without them the database would read
    // "id = 1 OR (name = 'alice' AND …)".
    let filters = vec![
        FilterCondition {
            column: "id".into(),
            operator: FilterOperator::Eq,
            value: Value::Integer(1),
            connector: None,
        },
        FilterCondition {
            column: "name".into(),
            operator: FilterOperator::Eq,
            value: Value::String("alice".into()),
            connector: Some(FilterConnector::Or),
        },
        FilterCondition {
            column: "id".into(),
            operator: FilterOperator::Gt,
            value: Value::Integer(0),
            connector: Some(FilterConnector::And),
        },
    ];
    let select = QueryExecutor::build_select_sql(
        "users",
        &columns,
        Some(filters),
        None,
        &DEFAULT,
        50,
        0,
        Some("and"),
    )
    .unwrap();
    assert!(
        select.contains("WHERE (\"id\" = 1 OR \"name\" = 'alice') AND \"id\" > 0"),
        "got: {select}"
    );
}
