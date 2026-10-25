//! The value search statement's shape (DB-19).

use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{
    BoundParameterLimit, DriverError, LegacySqlDialect, PaginationSyntax, SqlDialect, Value,
};

use super::*;

/// A dialect with a LIKE escape vocabulary, for the escape-clause case.
struct EscapingDialect;

impl SqlDialect for EscapingDialect {
    fn quote_ident(&self, name: &str) -> String {
        format!("\"{name}\"")
    }

    fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax {
        let _ = (limit, offset);
        PaginationSyntax {
            clause: String::new(),
            requires_order_by: false,
            order_by_fallback: None,
        }
    }

    fn like_pattern_escape(&self) -> Option<char> {
        Some('\\')
    }

    fn like_escape_clause(&self, escape_char: char) -> Option<&'static str> {
        let _ = escape_char;
        Some("ESCAPE '\\\\'")
    }

    fn try_format_sql_literal(&self, value: &Option<Value>) -> Result<String, DriverError> {
        Ok(match value {
            Some(Value::String(text)) => format!("'{text}'"),
            _ => "NULL".to_string(),
        })
    }

    fn parameter_placeholder(
        &self,
        index: usize,
        _data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        Ok(format!("?{index}"))
    }

    fn max_bound_parameters(&self) -> BoundParameterLimit {
        BoundParameterLimit::Dialect(999)
    }
}

fn columns(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn the_statement_counts_over_the_ored_columns() {
    let driver = MockDriver::new(
        "postgres",
        MockDriverOptions {
            parameterized_writes: true,
            ..Default::default()
        },
    );
    let dialect = LegacySqlDialect::new(driver.as_ref());
    let sql = build_search_statement(
        &dialect,
        "customers",
        &columns(&["name", "email"]),
        "acme",
        SearchMode::Contains,
    )
    .expect("the statement is expressible")
    .sql()
    .to_string();

    assert_eq!(
        sql, "SELECT COUNT(*) FROM \"customers\" WHERE \"name\" LIKE ?1 OR \"email\" LIKE ?1",
        "one value, bound once per column: {sql}"
    );
}

#[test]
fn the_exact_mode_compares_instead_of_searching() {
    let driver = MockDriver::new(
        "postgres",
        MockDriverOptions {
            parameterized_writes: true,
            ..Default::default()
        },
    );
    let dialect = LegacySqlDialect::new(driver.as_ref());
    let statement = build_search_statement(
        &dialect,
        "customers",
        &columns(&["name"]),
        "acme",
        SearchMode::Exact,
    )
    .expect("the statement is expressible");

    assert!(
        statement.sql().contains("\"name\" = ?1"),
        "exact is an equality: {}",
        statement.sql()
    );
    match statement {
        SearchStatement::Bound { params, .. } => {
            assert_eq!(
                serde_json::to_value(&params[0]).unwrap(),
                serde_json::json!("acme")
            );
        }
        other => panic!("expected the bound shape, got {:?}", other.sql()),
    }
}

#[test]
fn the_modes_shape_the_pattern() {
    let driver = MockDriver::new(
        "postgres",
        MockDriverOptions {
            parameterized_writes: true,
            ..Default::default()
        },
    );
    let dialect = LegacySqlDialect::new(driver.as_ref());

    let contains =
        build_search_statement(&dialect, "t", &columns(&["a"]), "ac", SearchMode::Contains)
            .expect("expressible");
    let prefix = build_search_statement(&dialect, "t", &columns(&["a"]), "ac", SearchMode::Prefix)
        .expect("expressible");
    let suffix = build_search_statement(&dialect, "t", &columns(&["a"]), "ac", SearchMode::Suffix)
        .expect("expressible");

    match contains {
        SearchStatement::Bound { params, .. } => {
            assert_eq!(
                serde_json::to_value(&params[0]).unwrap(),
                serde_json::json!("%ac%")
            );
        }
        _ => panic!("expected the bound shape"),
    }
    match prefix {
        SearchStatement::Bound { params, .. } => {
            assert_eq!(
                serde_json::to_value(&params[0]).unwrap(),
                serde_json::json!("ac%")
            );
        }
        _ => panic!("expected the bound shape"),
    }
    match suffix {
        SearchStatement::Bound { params, .. } => {
            assert_eq!(
                serde_json::to_value(&params[0]).unwrap(),
                serde_json::json!("%ac")
            );
        }
        _ => panic!("expected the bound shape"),
    }
}

#[test]
fn the_wildcards_in_the_value_are_escaped_only_where_the_dialect_escapes() {
    // A value of `50%` means "fifty percent", not "fifty and anything":
    // a dialect with an escape vocabulary keeps it literal.
    let statement = build_search_statement(
        &EscapingDialect,
        "t",
        &columns(&["a"]),
        "50%",
        SearchMode::Contains,
    )
    .expect("expressible");

    match statement {
        SearchStatement::Bound { params, sql } => {
            assert_eq!(
                serde_json::to_value(&params[0]).unwrap(),
                serde_json::json!("%50\\%%")
            );
            assert!(
                sql.contains("ESCAPE"),
                "the clause rides on the condition: {sql}"
            );
        }
        _ => panic!("expected the bound shape"),
    }

    // A dialect without an escape vocabulary passes the pattern through:
    // escaping there would degrade (an escaped `%` reads as a wildcard
    // again, and the query still succeeds — the answer lies).
    let driver = MockDriver::new("postgres", Default::default());
    let dialect = LegacySqlDialect::new(driver.as_ref());
    let statement =
        build_search_statement(&dialect, "t", &columns(&["a"]), "50%", SearchMode::Contains)
            .expect("expressible");
    match statement {
        SearchStatement::Inlined { sql } => {
            assert!(
                sql.contains("$datazen$%50%%$datazen$"),
                "the pattern travels as the driver's literal: {sql}"
            );
            assert!(
                !sql.contains("ESCAPE"),
                "no clause without a vocabulary: {sql}"
            );
        }
        other => panic!("the mock has no bound vocabulary: {:?}", other.sql()),
    }
}

#[tokio::test]
async fn an_empty_request_is_refused() {
    let driver = MockDriver::new("postgres", Default::default());
    let handle = datazen_driver_api::ConnectionHandle {
        id: "h".into(),
        pool_id: "p".into(),
    };
    let request = SearchTableValuesRequest {
        db_session_id: "session-1".into(),
        table: "customers".into(),
        columns: Vec::new(),
        value: "acme".into(),
        mode: SearchMode::Contains,
        database: None,
        schema: None,
    };

    let error = search_table_values(driver.as_ref(), &handle, "app", None, &request)
        .await
        .expect_err("no column is no search");

    assert!(
        error
            .to_string()
            .starts_with("grid.valueSearch.shapeMismatch"),
        "classified: {error}"
    );
}

#[tokio::test]
async fn the_count_is_read_from_the_drivers_answer() {
    use datazen_driver_api::mock_driver::MockDriverOptions;

    let driver = MockDriver::new(
        "postgres",
        MockDriverOptions {
            count_total: 42,
            ..Default::default()
        },
    );
    let handle = datazen_driver_api::ConnectionHandle {
        id: "h".into(),
        pool_id: "p".into(),
    };
    let request = SearchTableValuesRequest {
        db_session_id: "session-1".into(),
        table: "customers".into(),
        columns: columns(&["name"]),
        value: "acme".into(),
        mode: SearchMode::Contains,
        database: None,
        schema: None,
    };

    let result = search_table_values(driver.as_ref(), &handle, "app", None, &request)
        .await
        .expect("the search answers");

    assert_eq!(result.matches, 42);
    assert_eq!(result.searched_columns, columns(&["name"]));
}
