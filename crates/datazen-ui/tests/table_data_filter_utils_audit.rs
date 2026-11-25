//! T185 independent audit: fresh vectors for the filter-draft helpers.
//!
//! Different from the coder's tests: `notBetween`/`notIn` rows, a
//! three-token list with one blank, the pair-as-tokens edge, and the
//! unknown-operator completeness through the degradation rule.

use datazen_ui::components::filter::{FilterCondition, FilterConnector, FilterValue};
use datazen_ui::table_data_filter_utils::{
    filter_draft_equals_applied, is_complete_filter, value_input_for, FilterValueInputKind,
};

fn condition(column: &str, operator: &'static str, value: FilterValue) -> FilterCondition {
    FilterCondition::new(column, operator, value)
}

#[test]
fn audit_completeness_on_fresh_operators_and_edges() {
    // notBetween / notIn ride the same rows as between / in.
    assert!(is_complete_filter(&condition("age", "notBetween", FilterValue::Pair("10".into(), "20".into()))));
    assert!(!is_complete_filter(&condition("age", "notBetween", FilterValue::Pair("10".into(), "".into()))));
    assert!(is_complete_filter(&condition("city", "notIn", FilterValue::Tokens(vec!["berlin".into()]))));
    assert!(!is_complete_filter(&condition("city", "notIn", FilterValue::Tokens(vec![]))));
    // Three tokens with one blank is still complete.
    assert!(is_complete_filter(&condition(
        "city",
        "in",
        FilterValue::Tokens(vec!["berlin".into(), " ".into(), "oslo".into()])
    )));
    // The no-value rows are complete without a column value but not
    // without a column.
    assert!(is_complete_filter(&condition("flag", "isNotEmpty", FilterValue::None)));
    assert!(!is_complete_filter(&condition("", "isNotEmpty", FilterValue::None)));
    // A pair operator fed a token list of two filled bounds: complete
    // (React's array rule); one blank bound is not.
    assert!(is_complete_filter(&condition(
        "age",
        "between",
        FilterValue::Tokens(vec!["1".into(), "9".into()])
    )));
    assert!(!is_complete_filter(&condition(
        "age",
        "between",
        FilterValue::Tokens(vec!["1".into(), "  ".into()])
    )));
    assert!(!is_complete_filter(&condition(
        "age",
        "between",
        FilterValue::Tokens(vec!["1".into(), "9".into(), "20".into()])
    )));
    // An unknown operator degrades to single: a value makes it complete.
    assert_eq!(value_input_for("legacyOp"), FilterValueInputKind::Single);
    assert!(is_complete_filter(&condition("x", "legacyOp", FilterValue::Single("1".into()))));
    assert!(!is_complete_filter(&condition("x", "legacyOp", FilterValue::None)));
}

#[test]
fn audit_the_draft_comparison_through_a_connector_rewrite() {
    // Same conditions, one rewritten connector: not equal (React's
    // key-order-sensitive stringify sees the rewrite).
    let plain = condition("name", "eq", FilterValue::Single("alice".into()));
    let with_or = condition("name", "eq", FilterValue::Single("alice".into()))
        .connector(Some(FilterConnector::Or));
    assert!(!filter_draft_equals_applied(
        &[plain.clone()],
        FilterConnector::And,
        &[with_or.clone()],
        FilterConnector::And
    ));
    // Identical lists including connectors: equal.
    assert!(filter_draft_equals_applied(
        &[plain.clone(), with_or.clone()],
        FilterConnector::And,
        &[plain, with_or],
        FilterConnector::And
    ));
    // An empty draft against an applied list: not equal.
    assert!(!filter_draft_equals_applied(&[], FilterConnector::And, &[condition("a", "eq", FilterValue::None)], FilterConnector::And));
    // Empty against empty: equal.
    assert!(filter_draft_equals_applied(&[], FilterConnector::And, &[], FilterConnector::And));
}
