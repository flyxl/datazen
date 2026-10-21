//! Fixtures shared by the filter SQL tests.
//!
//! The dialect is declared here rather than in each test file: it is the one
//! place that says how a driver announces its `LIKE` escaping and how it
//! spells a dialect-specific operator, so the tests cannot drift apart on
//! those rules.

use super::*;
use datazen_driver_api::{BoundParameterLimit, DriverError, PaginationSyntax};

/// A dialect that stands on its own: no driver, no connection. It is also
/// the place where the `LIKE` escaping and the dialect operator are
/// declared, because those are exactly the two rules a driver owns.
pub(super) struct TestDialect {
    escape_char: Option<char>,
    escape_clause: Option<&'static str>,
    dialect_operator: Option<&'static str>,
}

pub(super) const PLAIN: TestDialect = TestDialect {
    escape_char: None,
    escape_clause: None,
    dialect_operator: None,
};

pub(super) const ESCAPING: TestDialect = TestDialect {
    escape_char: Some('!'),
    escape_clause: Some(" ESCAPE '!'"),
    dialect_operator: None,
};

/// An escape character without the matching `ESCAPE` clause: the dialect
/// treats the character literally, so an escaped `%` degrades back into a
/// wildcard.
pub(super) const HALF_CONFIGURED: TestDialect = TestDialect {
    escape_char: Some('!'),
    escape_clause: None,
    dialect_operator: None,
};

pub(super) const WITH_DIALECT_OPERATOR: TestDialect = TestDialect {
    escape_char: None,
    escape_clause: None,
    dialect_operator: Some("~"),
};

impl SqlDialect for TestDialect {
    fn quote_ident(&self, name: &str) -> String {
        format!("\"{name}\"")
    }

    fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax {
        PaginationSyntax {
            clause: format!("LIMIT {limit} OFFSET {offset}"),
            requires_order_by: false,
            order_by_fallback: None,
        }
    }

    fn try_format_sql_literal(&self, value: &Option<Value>) -> Result<String, DriverError> {
        Ok(match value {
            None | Some(Value::Null) => "NULL".to_string(),
            Some(Value::Bool(flag)) => (if *flag { "TRUE" } else { "FALSE" }).to_string(),
            Some(Value::Integer(number)) => number.to_string(),
            Some(Value::Float(number)) => number.to_string(),
            Some(Value::String(text) | Value::Timestamp(text)) => {
                format!("'{}'", text.replace('\'', "''"))
            }
            Some(Value::Bytes(_)) => "NULL".to_string(),
            Some(Value::Json(json)) => format!("'{}'", json.to_string().replace('\'', "''")),
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

    fn like_pattern_escape(&self) -> Option<char> {
        self.escape_char
    }

    fn like_escape_clause(&self, _escape_char: char) -> Option<&'static str> {
        self.escape_clause
    }

    fn filter_operator_sql(
        &self,
        _operator: FilterOperator,
        column_expr: &str,
        literal: &str,
    ) -> Option<String> {
        self.dialect_operator
            .map(|operator| format!("{column_expr} {operator} {literal}"))
    }
}

/// A condition without a connector.
pub(super) fn condition(column: &str, operator: FilterOperator, value: Value) -> FilterCondition {
    FilterCondition {
        column: column.to_string(),
        operator,
        value,
        connector: None,
    }
}

/// A condition that carries its own connector.
pub(super) fn linked(
    column: &str,
    operator: FilterOperator,
    value: Value,
    connector: Option<FilterConnector>,
) -> FilterCondition {
    FilterCondition {
        column: column.to_string(),
        operator,
        value,
        connector,
    }
}

/// Render one condition and unwrap the refusal, for the tests that expect a
/// fragment.
pub(super) fn rendered(
    condition: &FilterCondition,
    dialect: &dyn SqlDialect,
) -> Result<String, FilterRejection> {
    format_condition(condition, dialect)
}

/// A rendered fragment together with the connector that joins it to the
/// fragment before it.
pub(super) fn part(
    sql: &str,
    connector: Option<FilterConnector>,
) -> (String, Option<FilterConnector>) {
    (sql.to_string(), connector)
}
