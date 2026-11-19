//! T185: the filter-draft helpers — the completeness predicate that
//! mirrors the backend, the value-input kind table, and the draft
//! comparison.

use crate::components::filter::{FilterCondition, FilterConnector, FilterValue};
use crate::table_data_filter_utils::{
    clone_filters, filter_draft_equals_applied, is_complete_filter, value_input_for,
    FilterValueInputKind,
};

fn condition(column: &str, operator: &'static str, value: FilterValue) -> FilterCondition {
    FilterCondition::new(column, operator, value)
}

#[test]
fn the_value_input_table_matches_the_operator_rows() {
    for operator in ["isNull", "isNotNull", "isEmpty", "isNotEmpty"] {
        assert_eq!(value_input_for(operator), FilterValueInputKind::None, "{operator} carries no value");
    }
    for operator in ["between", "notBetween"] {
        assert_eq!(value_input_for(operator), FilterValueInputKind::Pair, "{operator} takes a pair");
    }
    for operator in ["in", "notIn"] {
        assert_eq!(value_input_for(operator), FilterValueInputKind::Tokens, "{operator} takes a list");
    }
    for operator in ["eq", "ne", "gt", "like", "contains", "regex", "jsonContains"] {
        assert_eq!(value_input_for(operator), FilterValueInputKind::Single, "{operator} takes one literal");
    }
    // An operator this build does not know degrades to a single field.
    assert_eq!(value_input_for("frozenOperator"), FilterValueInputKind::Single);
}

#[test]
fn completeness_follows_the_backend_predicate() {
    // The no-value operators are complete on their own — even with the
    // value missing.
    assert!(is_complete_filter(&condition("status", "isNull", FilterValue::None)));
    assert!(is_complete_filter(&condition("status", "isEmpty", FilterValue::None)));

    // A missing column is never complete.
    assert!(!is_complete_filter(&condition("", "isNull", FilterValue::None)));

    // A single literal: only the empty string counts as missing.
    assert!(is_complete_filter(&condition("name", "eq", FilterValue::Single("alice".into()))));
    assert!(!is_complete_filter(&condition("name", "eq", FilterValue::Single(String::new()))));
    assert!(is_complete_filter(&condition("name", "eq", FilterValue::Single(" ".into()))), "a blank-but-nonempty string is a value");
    assert!(!is_complete_filter(&condition("name", "eq", FilterValue::None)));

    // A pair needs exactly two filled bounds.
    assert!(is_complete_filter(&condition("age", "between", FilterValue::Pair("1".into(), "9".into()))));
    assert!(!is_complete_filter(&condition("age", "between", FilterValue::Pair("1".into(), " ".into()))));
    assert!(!is_complete_filter(&condition("age", "between", FilterValue::Pair("1".into(), String::new()))));
    // The persisted string form: two comma parts, both filled.
    assert!(is_complete_filter(&condition("age", "between", FilterValue::Single("1,9".into()))));
    assert!(!is_complete_filter(&condition("age", "between", FilterValue::Single("1,".into()))));
    assert!(!is_complete_filter(&condition("age", "between", FilterValue::Single("1,9,20".into()))));
    assert!(!is_complete_filter(&condition("age", "between", FilterValue::Single(String::new()))));

    // A set needs at least one value.
    assert!(is_complete_filter(&condition("status", "in", FilterValue::Tokens(vec!["paid".into()]))));
    assert!(!is_complete_filter(&condition("status", "in", FilterValue::Tokens(vec![]))));
    assert!(is_complete_filter(&condition("status", "in", FilterValue::Single("paid, void".into()))));
    assert!(!is_complete_filter(&condition("status", "in", FilterValue::Single(", ,".into()))));
    assert!(!is_complete_filter(&condition("status", "in", FilterValue::None)));
}

#[test]
fn the_draft_comparison_is_order_sensitive_and_logic_aware() {
    let a = condition("name", "eq", FilterValue::Single("alice".into()))
        .connector(Some(FilterConnector::Or));
    let b = condition("name", "eq", FilterValue::Single("alice".into()));
    let c = condition("age", "gt", FilterValue::Single("30".into()));

    // Same logic, same order: equal.
    assert!(filter_draft_equals_applied(
        &[a.clone(), c.clone()],
        FilterConnector::And,
        &[a.clone(), c.clone()],
        FilterConnector::And
    ));
    // A connector rewrite changes the draft (React's spread comment).
    assert!(!filter_draft_equals_applied(
        &[a.clone()],
        FilterConnector::And,
        &[b.clone()],
        FilterConnector::And
    ));
    // Order is part of the comparison.
    assert!(!filter_draft_equals_applied(
        &[a.clone(), c.clone()],
        FilterConnector::And,
        &[c.clone(), a.clone()],
        FilterConnector::And
    ));
    // The wide logic is part of it too.
    assert!(!filter_draft_equals_applied(
        &[a.clone()],
        FilterConnector::And,
        &[a.clone()],
        FilterConnector::Or
    ));
    // Clone keeps the conditions intact.
    let cloned = clone_filters(&[a.clone(), c.clone()]);
    assert_eq!(cloned, vec![a, c]);
}
