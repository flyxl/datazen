//! Independent audit for T620: pure `useNamedViewMenu` decisions.
//!
//! Pin the scope edge values, raw/filter branch selection, unknown-operator
//! refusal order and dirty-flag field coverage the coder does not drive.

use serde_json::{json, Value};

use crate::components::filter::{FilterLogic, ViewSort};
use crate::views::use_named_view_menu::{
    apply_plan, derive_scope, draft_is_dirty, filter_draft_equals_applied, menu_present,
    ApplyAction, NamedViewFilterVm, SavedViewVm,
};

fn filter(column: &str, operator: &str, value: Option<Value>) -> NamedViewFilterVm {
    NamedViewFilterVm { column: column.to_owned(), operator: operator.to_owned(), value }
}

fn view() -> SavedViewVm {
    SavedViewVm {
        filters: vec![filter("id", "eq", Some(json!(1)))],
        filter_logic: FilterLogic::And,
        sorts: vec![ViewSort { column: "name".to_owned(), descending: false }],
        visible_columns: Some(vec!["id".to_owned()]),
        ..Default::default()
    }
}

#[test]
fn scope_edge_values_follow_react_truthiness() {
    let scope = derive_scope(Some("  "), "  ", Some(""), "").expect("whitespace connection id is truthy");
    assert_eq!(scope.connection_id, "  ");
    assert_eq!(scope.database.as_deref(), Some("  "), "whitespace database is kept");
    assert_eq!(scope.schema.as_deref(), Some(""), "empty schema is null-coalesced, not emptied");
    assert_eq!(scope.table, "");
    assert!(menu_present(Some(&scope), true));
}

#[test]
fn raw_where_bypasses_the_unknown_operator_guard() {
    let mut v = view();
    v.raw_where = Some("id > 5".to_owned());
    v.filters = vec![filter("id", "unknown-op", Some(json!(1)))];
    let plan = apply_plan(&v);
    assert!(!plan.refused, "audit: a saved raw predicate bypasses the filter operator guard");
    assert_eq!(plan.actions.first(), Some(&ApplyAction::ApplyRawWhere("id > 5".to_owned())));
    assert!(!plan.actions.iter().any(|a| matches!(a, ApplyAction::SetFilters(_, _))));
}

#[test]
fn empty_raw_where_falls_back_to_filters() {
    let mut v = view();
    v.raw_where = Some(String::new());
    let plan = apply_plan(&v);
    assert!(!plan.refused);
    assert!(plan.actions.iter().any(|a| matches!(a, ApplyAction::SetFilters(_, _))), "audit: empty raw is the filter path");
}

#[test]
fn equality_compares_every_filter_field_and_length() {
    let a = vec![filter("id", "eq", Some(json!(1)))];
    assert!(filter_draft_equals_applied(&[], FilterLogic::And, &[], FilterLogic::And), "audit: two empty drafts are equal");
    assert!(!filter_draft_equals_applied(&a, FilterLogic::And, &[], FilterLogic::And), "audit: length differs");
    assert!(!filter_draft_equals_applied(
        &[filter("id", "eq", Some(json!(1)))],
        FilterLogic::And,
        &[filter("other", "eq", Some(json!(1)))],
        FilterLogic::And,
    ), "audit: column differs");
    assert!(!filter_draft_equals_applied(
        &[filter("id", "eq", Some(json!(1)))],
        FilterLogic::And,
        &[filter("id", "ne", Some(json!(1)))],
        FilterLogic::And,
    ), "audit: operator differs");
    assert!(!filter_draft_equals_applied(
        &[filter("id", "eq", None)],
        FilterLogic::And,
        &[filter("id", "eq", Some(Value::Null))],
        FilterLogic::And,
    ), "audit: absent and explicit NULL differ");
}

#[test]
fn dirty_flag_is_the_exact_negation() {
    let applied = vec![filter("id", "eq", Some(json!(1)))];
    for (draft, logic) in [
        (applied.clone(), FilterLogic::And),
        (vec![filter("id", "eq", Some(json!(2)))], FilterLogic::And),
        (applied.clone(), FilterLogic::Or),
    ] {
        assert_eq!(
            draft_is_dirty(&draft, logic, &applied, FilterLogic::And),
            !filter_draft_equals_applied(&draft, logic, &applied, FilterLogic::And),
            "audit: dirty is the negation of equality"
        );
    }
}
