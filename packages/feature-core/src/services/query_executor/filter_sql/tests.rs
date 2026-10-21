//! Tests for the filter SQL module.
//!
//! They live out of line so the module itself stays readable; the filter
//! rules are the part of the query executor that has to be read carefully.

use super::*;
use datazen_driver_api::filters::{DIALECT_FILTER_OPERATORS, SHARED_FILTER_OPERATORS};
use datazen_driver_api::{BoundParameterLimit, DriverError, PaginationSyntax};

/// A dialect that stands on its own: no driver, no connection. It is also
/// the place where the `LIKE` escaping and the dialect operator are
/// declared, because those are exactly the two rules a driver owns.
struct TestDialect {
    escape_char: Option<char>,
    escape_clause: Option<&'static str>,
    dialect_operator: Option<&'static str>,
}

const PLAIN: TestDialect = TestDialect {
    escape_char: None,
    escape_clause: None,
    dialect_operator: None,
};

const ESCAPING: TestDialect = TestDialect {
    escape_char: Some('!'),
    escape_clause: Some(" ESCAPE '!'"),
    dialect_operator: None,
};

const HALF_CONFIGURED: TestDialect = TestDialect {
    escape_char: Some('!'),
    escape_clause: None,
    dialect_operator: None,
};

const WITH_DIALECT_OPERATOR: TestDialect = TestDialect {
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

fn condition(column: &str, operator: FilterOperator, value: Value) -> FilterCondition {
    FilterCondition {
        column: column.to_string(),
        operator,
        value,
        connector: None,
    }
}

fn rendered(
    condition: &FilterCondition,
    dialect: &dyn SqlDialect,
) -> Result<String, FilterRejection> {
    format_condition(condition, dialect)
}

#[test]
fn legacy_ten_operators_render_byte_identical() {
    let text = |value: &str| Value::String(value.to_string());
    assert_eq!(
        rendered(&condition("name", FilterOperator::Eq, text("x")), &PLAIN).unwrap(),
        "\"name\" = 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Ne, text("x")), &PLAIN).unwrap(),
        "\"name\" != 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Gt, text("x")), &PLAIN).unwrap(),
        "\"name\" > 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Lt, text("x")), &PLAIN).unwrap(),
        "\"name\" < 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Gte, text("x")), &PLAIN).unwrap(),
        "\"name\" >= 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Lte, text("x")), &PLAIN).unwrap(),
        "\"name\" <= 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Like, text("x%")), &PLAIN).unwrap(),
        "\"name\" LIKE 'x%'"
    );
    assert_eq!(
        rendered(
            &condition(
                "id",
                FilterOperator::In,
                Value::Json(serde_json::json!([1, 2, 3]))
            ),
            &PLAIN
        )
        .unwrap(),
        "\"id\" IN ('1', '2', '3')"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::In, text("a, b")), &PLAIN).unwrap(),
        "\"name\" IN ('a', 'b')"
    );
    assert_eq!(
        rendered(
            &condition("deleted_at", FilterOperator::IsNull, Value::Null),
            &PLAIN
        )
        .unwrap(),
        "\"deleted_at\" IS NULL"
    );
    assert_eq!(
        rendered(
            &condition("deleted_at", FilterOperator::IsNotNull, Value::Null),
            &PLAIN
        )
        .unwrap(),
        "\"deleted_at\" IS NOT NULL"
    );
}

#[test]
fn in_comma_string_with_only_blank_parts_is_rejected() {
    let rejection = rendered(
        &condition("name", FilterOperator::In, Value::String(" , ".into())),
        &PLAIN,
    )
    .expect_err("a blank list must not become an empty IN");
    assert!(matches!(rejection, FilterRejection::Incomplete { .. }));
}

#[test]
fn not_in_mirrors_in_element_formatting() {
    let in_sql = rendered(
        &condition(
            "id",
            FilterOperator::In,
            Value::Json(serde_json::json!([1, 2, 3])),
        ),
        &PLAIN,
    )
    .unwrap();
    let not_in_sql = rendered(
        &condition(
            "id",
            FilterOperator::NotIn,
            Value::Json(serde_json::json!([1, 2, 3])),
        ),
        &PLAIN,
    )
    .unwrap();
    assert_eq!(not_in_sql.replace(" NOT IN ", " IN "), in_sql);
}

#[test]
fn between_renders_two_literals_in_order() {
    assert_eq!(
        rendered(
            &condition(
                "score",
                FilterOperator::Between,
                Value::Json(serde_json::json!([1, 5]))
            ),
            &PLAIN
        )
        .unwrap(),
        "\"score\" BETWEEN '1' AND '5'"
    );
    // The host does not swap the bounds: a reversed range is the user's
    // input, and the database answers it honestly.
    assert_eq!(
        rendered(
            &condition(
                "score",
                FilterOperator::Between,
                Value::Json(serde_json::json!([5, 1]))
            ),
            &PLAIN
        )
        .unwrap(),
        "\"score\" BETWEEN '5' AND '1'"
    );
}

#[test]
fn between_comma_string_splits_on_first_comma_only() {
    assert_eq!(
        rendered(
            &condition(
                "created_at",
                FilterOperator::Between,
                Value::String("2024-01-01,2024-02-01".into())
            ),
            &PLAIN
        )
        .unwrap(),
        "\"created_at\" BETWEEN '2024-01-01' AND '2024-02-01'"
    );
    let rejection = rendered(
        &condition(
            "created_at",
            FilterOperator::Between,
            Value::String("a,b,c".into()),
        ),
        &PLAIN,
    )
    .expect_err("three comma separated parts are not two bounds");
    assert!(matches!(rejection, FilterRejection::Incomplete { .. }));
}

#[test]
fn contains_escapes_wildcards_and_escape_char() {
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::Contains,
                Value::String("50%".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE '%50!%%' ESCAPE '!'"
    );
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::Contains,
                Value::String("a_b".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE '%a!_b%' ESCAPE '!'"
    );
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::Contains,
                Value::String("a!b".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE '%a!!b%' ESCAPE '!'"
    );
}

#[test]
fn starts_with_and_ends_with_put_wildcard_on_one_side() {
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::StartsWith,
                Value::String("a_".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE 'a!_%' ESCAPE '!'"
    );
    assert_eq!(
        rendered(
            &condition("name", FilterOperator::EndsWith, Value::String("a_".into())),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE '%a!_' ESCAPE '!'"
    );
}

#[test]
fn not_contains_uses_not_like_with_same_escaping() {
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::NotContains,
                Value::String("50%".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" NOT LIKE '%50!%%' ESCAPE '!'"
    );
}

#[test]
fn is_empty_compares_to_empty_string_only() {
    assert_eq!(
        rendered(
            &condition("name", FilterOperator::IsEmpty, Value::Null),
            &PLAIN
        )
        .unwrap(),
        "\"name\" = ''"
    );
    assert_eq!(
        rendered(
            &condition("name", FilterOperator::IsNotEmpty, Value::Null),
            &PLAIN
        )
        .unwrap(),
        "\"name\" <> ''"
    );
}

#[test]
fn regex_without_driver_support_is_rejected_with_contract_prefix() {
    let rejection = rendered(
        &condition("name", FilterOperator::Regex, Value::String("^a".into())),
        &PLAIN,
    )
    .expect_err("a dialect operator nobody implements must be refused");
    assert!(matches!(
        rejection,
        FilterRejection::UnsupportedOperator { .. }
    ));
    assert!(rejection
        .contract_message()
        .starts_with("grid.filter.unsupportedOperator: "));
}

#[test]
fn regex_with_driver_support_uses_driver_sql_verbatim() {
    assert_eq!(
        rendered(
            &condition("name", FilterOperator::Regex, Value::String("^a".into())),
            &WITH_DIALECT_OPERATOR
        )
        .unwrap(),
        "\"name\" ~ '^a'"
    );
}

#[test]
fn incomplete_condition_is_rejected_not_silently_dropped() {
    let rejection = rendered(
        &condition("id", FilterOperator::Eq, Value::String(String::new())),
        &PLAIN,
    )
    .expect_err("an incomplete condition must be refused, not skipped");
    assert!(matches!(rejection, FilterRejection::Incomplete { .. }));
    assert!(rejection
        .contract_message()
        .starts_with("grid.filter.incomplete: "));
}

#[test]
fn complete_and_incomplete_mixed_list_reports_the_offending_column() {
    let filters = vec![
        condition("name", FilterOperator::Eq, Value::String("alice".into())),
        condition("id", FilterOperator::Eq, Value::String(String::new())),
    ];
    let rejection = validate_filters(SHARED_FILTER_OPERATORS, &filters)
        .expect_err("the incomplete condition must be named");
    assert_eq!(
        rejection.contract_message(),
        "grid.filter.incomplete: column \"id\" needs a value (operator 'eq')"
    );
}

#[test]
fn filter_join_defaults_to_and() {
    assert_eq!(filter_join(None), " AND ");
    assert_eq!(filter_join(Some("and")), " AND ");
    assert_eq!(filter_join(Some("or")), " OR ");
}

#[test]
fn multibyte_pattern_is_not_byte_sliced() {
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::Contains,
                Value::String("中文🙂%".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE '%中文🙂!%%' ESCAPE '!'"
    );
}

#[test]
fn single_quote_in_value_is_left_to_driver_formatter() {
    assert_eq!(
        rendered(
            &condition("name", FilterOperator::Eq, Value::String("O'Brien".into())),
            &PLAIN
        )
        .unwrap(),
        "\"name\" = 'O''Brien'"
    );
}

#[test]
fn literal_arity_mismatch_is_rejected_for_between_and_in() {
    let one = rendered(
        &condition(
            "score",
            FilterOperator::Between,
            Value::Json(serde_json::json!([1])),
        ),
        &PLAIN,
    );
    assert!(one.is_err());
    let three = rendered(
        &condition(
            "score",
            FilterOperator::Between,
            Value::Json(serde_json::json!([1, 2, 3])),
        ),
        &PLAIN,
    );
    assert!(three.is_err());
    let empty_set = rendered(
        &condition("id", FilterOperator::In, Value::Json(serde_json::json!([]))),
        &PLAIN,
    );
    assert!(empty_set.is_err());
}

#[test]
fn like_escape_config_rejects_half_configured_pair() {
    let rejection = like_escape_config(&HALF_CONFIGURED)
        .expect_err("an escape character without a clause must be refused");
    assert!(matches!(
        rejection,
        FilterRejection::InvalidEscapeConfiguration { escape_char: '!' }
    ));
    assert!(rejection
        .contract_message()
        .starts_with("grid.filter.unsupportedOperator: "));
}

#[test]
fn like_escape_config_none_none_is_identity() {
    let config = like_escape_config(&PLAIN).unwrap();
    assert_eq!(config.escape_char, None);
    assert_eq!(config.clause, None);
    // Without escaping the pattern is the plain text again, so a `%` the
    // user typed stays a wildcard.
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::Contains,
                Value::String("50%".into())
            ),
            &PLAIN
        )
        .unwrap(),
        "\"name\" LIKE '%50%%'"
    );
}

#[test]
fn validate_filters_rejects_operator_outside_the_whitelist() {
    let filters = vec![condition(
        "name",
        FilterOperator::Regex,
        Value::String("^a".into()),
    )];
    let rejection = validate_filters(SHARED_FILTER_OPERATORS, &filters)
        .expect_err("a dialect operator is not in the shared whitelist");
    assert!(rejection
        .contract_message()
        .starts_with("grid.filter.unsupportedOperator: "));
    for operator in DIALECT_FILTER_OPERATORS {
        assert!(!SHARED_FILTER_OPERATORS.contains(operator));
    }
}

#[test]
fn validate_filters_accepts_the_shared_operators() {
    let filters = vec![
        condition("name", FilterOperator::Eq, Value::String("alice".into())),
        condition(
            "id",
            FilterOperator::In,
            Value::Json(serde_json::json!([1, 2])),
        ),
        condition("deleted_at", FilterOperator::IsNull, Value::Null),
    ];
    assert!(validate_filters(SHARED_FILTER_OPERATORS, &filters).is_ok());
}

#[test]
fn overlong_regex_pattern_is_refused() {
    let pattern = "a".repeat(MAX_REGEX_PATTERN_LENGTH + 1);
    let filters = vec![condition(
        "name",
        FilterOperator::Regex,
        Value::String(pattern),
    )];
    // A dialect operator only reaches the length check on a driver that
    // declared it.
    let mut supported = SHARED_FILTER_OPERATORS.to_vec();
    supported.push(FilterOperator::Regex);
    let rejection =
        validate_filters(&supported, &filters).expect_err("an overlong pattern must be refused");
    assert!(rejection
        .contract_message()
        .starts_with("grid.filter.incomplete: "));
}

fn part(sql: &str, connector: Option<FilterConnector>) -> (String, Option<FilterConnector>) {
    (sql.to_string(), connector)
}

#[test]
fn join_filters_all_connectors_equal_is_byte_identical_to_today() {
    let parts = vec![
        part("A", Some(FilterConnector::And)),
        part("B", Some(FilterConnector::And)),
        part("C", Some(FilterConnector::And)),
    ];
    assert_eq!(
        join_filters(&parts, Some("and")),
        parts
            .iter()
            .map(|(sql, _)| sql.as_str())
            .collect::<Vec<_>>()
            .join(" AND ")
    );

    let parts = vec![part("A", None), part("B", None), part("C", None)];
    assert_eq!(
        join_filters(&parts, Some("or")),
        parts
            .iter()
            .map(|(sql, _)| sql.as_str())
            .collect::<Vec<_>>()
            .join(" OR ")
    );
}

#[test]
fn join_filters_mixed_parenthesizes_and_runs() {
    // The rows are read left to right: "A OR B AND C" asks for
    // "(A OR B) AND C". SQL binds AND first, so the group has to be
    // written out — otherwise the database answers a different question.
    assert_eq!(
        join_filters(
            &[
                part("A", None),
                part("B", Some(FilterConnector::Or)),
                part("C", Some(FilterConnector::And)),
            ],
            Some("and")
        ),
        "(A OR B) AND C"
    );
    assert_eq!(
        join_filters(
            &[
                part("A", None),
                part("B", Some(FilterConnector::Or)),
                part("C", Some(FilterConnector::And)),
                part("D", Some(FilterConnector::Or)),
            ],
            Some("and")
        ),
        "(A OR B) AND C OR D"
    );
    // A single fragment never gets parentheses.
    assert_eq!(join_filters(&[part("A", None)], Some("and")), "A");
}

#[test]
fn join_filters_first_entry_connector_is_ignored() {
    let with_connector = join_filters(
        &[
            part("A", Some(FilterConnector::Or)),
            part("B", Some(FilterConnector::And)),
        ],
        Some("and"),
    );
    let without_connector = join_filters(
        &[part("A", None), part("B", Some(FilterConnector::And))],
        Some("and"),
    );
    assert_eq!(with_connector, without_connector);
}

#[test]
fn join_filters_empty_is_empty_string() {
    assert_eq!(join_filters(&[], Some("and")), "");
}

#[test]
fn join_filters_survives_a_dropped_fragment() {
    // A driver may answer with an empty fragment; the connector has to
    // travel with the fragment it belongs to, or every connector after the
    // dropped one shifts by a position.
    let parts = vec![
        part("A", None),
        part("", Some(FilterConnector::Or)),
        part("C", Some(FilterConnector::And)),
    ];
    let kept: Vec<(String, Option<FilterConnector>)> = parts
        .into_iter()
        .filter(|(sql, _)| !sql.is_empty())
        .collect();
    assert_eq!(join_filters(&kept, Some("and")), "A AND C");
}

#[test]
fn render_filter_parts_keeps_fragments_and_connectors_paired() {
    let filters = vec![
        FilterCondition {
            column: "a".into(),
            operator: FilterOperator::Eq,
            value: Value::String("1".into()),
            connector: None,
        },
        FilterCondition {
            column: "b".into(),
            operator: FilterOperator::Eq,
            value: Value::String("2".into()),
            connector: Some(FilterConnector::Or),
        },
    ];
    let parts = render_filter_parts(&filters, &PLAIN).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].1, None);
    assert_eq!(parts[1].1, Some(FilterConnector::Or));
    assert_eq!(parts[0].0, "\"a\" = '1'");
    assert_eq!(parts[1].0, "\"b\" = '2'");
}

#[test]
fn filter_is_complete_forwards_the_shared_predicate() {
    let complete = condition("name", FilterOperator::Eq, Value::String("x".into()));
    let incomplete = condition("name", FilterOperator::Eq, Value::String(String::new()));
    assert!(filter_is_complete(&complete));
    assert!(!filter_is_complete(&incomplete));
}
