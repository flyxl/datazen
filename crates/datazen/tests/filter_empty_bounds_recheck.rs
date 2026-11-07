//! Independent re-verification of the shared layer's empty-bound rule (08 §3.3.1).
//!
//! This file is an **integration** test, so it drives the public API of the
//! crate (`format_condition`, `filter_is_complete`, `incomplete_reason`) instead
//! of the crate-internal helpers the unit tests reach for. That matters here:
//! the claim under test is about the *shipped* behaviour, not about one private
//! function.
//!
//! It reads the same canonical case table the host-side parity test reads
//! (`src/stores/tableData/__tests__/filterCompletenessParity.test.ts`), so the
//! two sides judge the same inputs against the same expectation.

use datazen::services::query_executor::filter_sql::{
    filter_is_complete, format_condition, incomplete_reason, FilterRejection,
};
use datazen_driver_api::dialect::SqlDialect;
use datazen_driver_api::filters::{FilterCondition, FilterOperator};
use datazen_driver_api::{BoundParameterLimit, DriverError, PaginationSyntax, Value};
use serde_json::Value as JsonValue;
use std::path::PathBuf;

/// A dialect that stands on its own — the same shape as the crate's own
/// fixture, but declared here so this test does not depend on a `pub(super)`
/// helper.
struct PlainDialect;

impl SqlDialect for PlainDialect {
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
}

/// One row of the canonical table.
#[derive(Debug)]
struct Case {
    id: String,
    operator: FilterOperator,
    value_kind: String,
    value: JsonValue,
    expected_complete: bool,
}

fn load_cases() -> Vec<Case> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/filter_completeness_cases.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let parsed: JsonValue = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("cannot parse the table: {error}"));
    let rows = parsed["cases"]
        .as_array()
        .expect("the table has a cases array");
    rows.iter()
        .map(|row| Case {
            id: row["id"].as_str().expect("a case id").to_string(),
            operator: match row["operator"].as_str().expect("an operator") {
                "between" => FilterOperator::Between,
                "notBetween" => FilterOperator::NotBetween,
                other => panic!("unknown operator: {other}"),
            },
            value_kind: row["value_kind"]
                .as_str()
                .expect("a value kind")
                .to_string(),
            value: row["value"].clone(),
            expected_complete: row["expected_complete"].as_bool().expect("a verdict"),
        })
        .collect()
}

fn condition(case: &Case) -> FilterCondition {
    let value = match case.value_kind.as_str() {
        "json_array" => Value::Json(case.value.clone()),
        "string" => Value::String(case.value.as_str().expect("a string value").to_string()),
        other => panic!("unknown value kind: {other}"),
    };
    FilterCondition {
        column: "score".to_string(),
        operator: case.operator,
        value,
        connector: None,
    }
}

#[test]
fn the_shared_layer_agrees_with_the_canonical_completeness_table() {
    let cases = load_cases();
    assert!(cases.len() >= 30, "the table must cover every shape");

    for case in &cases {
        let condition = condition(case);

        let complete = filter_is_complete(&condition);
        assert_eq!(
            complete, case.expected_complete,
            "{}: filter_is_complete disagrees with the table",
            case.id
        );

        let reason = incomplete_reason(&condition);
        assert_eq!(
            reason.is_none(),
            case.expected_complete,
            "{}: incomplete_reason disagrees with filter_is_complete",
            case.id
        );

        // The renderer and the predicate must not drift apart: an incomplete
        // condition must be refused, a complete one must render.
        let rendered = format_condition(&condition, &PlainDialect);
        if case.expected_complete {
            assert!(
                rendered.is_ok(),
                "{}: a complete condition must render, got {rendered:?}",
                case.id
            );
        } else {
            let rejection = rendered.expect_err(&format!(
                "{}: an incomplete condition must be refused",
                case.id
            ));
            assert!(
                matches!(rejection, FilterRejection::Incomplete { .. }),
                "{}: the refusal must be an incompleteness, got {rejection:?}",
                case.id
            );
            let message = rejection.contract_message();
            assert!(
                message.starts_with("grid.filter.incomplete: "),
                "{}: the refusal must carry the contract prefix, got {message}",
                case.id
            );
        }
    }
}

#[test]
fn the_refusal_names_the_missing_bound() {
    let lower = FilterCondition {
        column: "score".to_string(),
        operator: FilterOperator::Between,
        value: Value::Json(serde_json::json!(["", "10"])),
        connector: None,
    };
    let message = format_condition(&lower, &PlainDialect)
        .expect_err("an empty lower bound is not a bound")
        .contract_message();
    assert!(message.contains("lower bound"), "got: {message}");

    let upper = FilterCondition {
        column: "score".to_string(),
        operator: FilterOperator::Between,
        value: Value::Json(serde_json::json!(["10", ""])),
        connector: None,
    };
    let message = format_condition(&upper, &PlainDialect)
        .expect_err("an empty upper bound is not a bound")
        .contract_message();
    assert!(message.contains("upper bound"), "got: {message}");
    // The silent wrong result the rule exists to prevent: this is the fragment
    // the old array branch produced.
    assert!(
        !message.contains("BETWEEN '10' AND ''"),
        "the refusal must not quote the silent fragment: {message}"
    );
}

#[test]
fn a_zero_and_a_false_are_values_not_emptiness() {
    let zero = FilterCondition {
        column: "score".to_string(),
        operator: FilterOperator::Between,
        value: Value::Json(serde_json::json!([0, 10])),
        connector: None,
    };
    assert!(filter_is_complete(&zero));
    assert_eq!(
        format_condition(&zero, &PlainDialect).unwrap(),
        "\"score\" BETWEEN '0' AND '10'"
    );

    let flags = FilterCondition {
        column: "score".to_string(),
        operator: FilterOperator::Between,
        value: Value::Json(serde_json::json!([false, true])),
        connector: None,
    };
    assert!(filter_is_complete(&flags));
    assert_eq!(
        format_condition(&flags, &PlainDialect).unwrap(),
        "\"score\" BETWEEN 'false' AND 'true'"
    );

    // The string form has the same rule.
    let zero_text = FilterCondition {
        column: "score".to_string(),
        operator: FilterOperator::Between,
        value: Value::String("0,10".to_string()),
        connector: None,
    };
    assert!(filter_is_complete(&zero_text));
    assert_eq!(
        format_condition(&zero_text, &PlainDialect).unwrap(),
        "\"score\" BETWEEN '0' AND '10'"
    );
}

#[test]
fn the_negated_range_judges_the_same_way() {
    for value in [
        serde_json::json!(["10", ""]),
        serde_json::json!(["", "10"]),
        serde_json::json!(["10", "   "]),
        serde_json::json!([null, "10"]),
        serde_json::json!([10, null]),
    ] {
        let condition = FilterCondition {
            column: "score".to_string(),
            operator: FilterOperator::NotBetween,
            value: Value::Json(value.clone()),
            connector: None,
        };
        assert!(
            !filter_is_complete(&condition),
            "notBetween with {value:?} must be incomplete"
        );
        assert!(format_condition(&condition, &PlainDialect).is_err());
    }

    let text = FilterCondition {
        column: "score".to_string(),
        operator: FilterOperator::NotBetween,
        value: Value::String("10,".to_string()),
        connector: None,
    };
    assert!(!filter_is_complete(&text));
    assert!(format_condition(&text, &PlainDialect).is_err());
}
