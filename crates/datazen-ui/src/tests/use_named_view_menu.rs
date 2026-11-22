//! T620: pure `useNamedViewMenu` decisions — scope guards, apply path order,
//! raw-WHERE branch, unknown-operator refusal, sort/column propagation and the
//! dirty flag. No store, IPC or timer.

use serde_json::json;

use crate::components::filter::{FilterLogic, ViewSort};
use crate::views::use_named_view_menu::{
    apply_plan, derive_scope, draft_is_dirty, filter_draft_equals_applied, menu_present,
    ApplyAction, NamedViewFilterVm, SavedViewVm,
};

fn filter(column: &str, operator: &str, value: Option<serde_json::Value>) -> NamedViewFilterVm {
    NamedViewFilterVm {
        column: column.to_owned(),
        operator: operator.to_owned(),
        value,
    }
}

fn view() -> SavedViewVm {
    SavedViewVm {
        filters: vec![filter("id", "eq", Some(json!(1))), filter("name", "like", Some(json!("a")))],
        filter_logic: FilterLogic::Or,
        sorts: vec![ViewSort { column: "name".to_owned(), descending: true }],
        visible_columns: Some(vec!["id".to_owned(), "name".to_owned()]),
        ..Default::default()
    }
}

#[test]
fn scope_is_persistent_and_null_without_connection() {
    let scope = derive_scope(Some("conn-1"), "app", Some("public"), "orders")
        .expect("a persistent connection id gives a scope");
    assert_eq!(scope.connection_id, "conn-1");
    assert_eq!(scope.database.as_deref(), Some("app"));
    assert_eq!(scope.schema.as_deref(), Some("public"));
    assert_eq!(scope.table, "orders");

    let empty_db = derive_scope(Some("conn-1"), "", None, "orders").expect("scope with no database");
    assert_eq!(empty_db.database, None, "an empty database is null in the scope");
    assert!(derive_scope(None, "app", None, "orders").is_none());
    assert!(derive_scope(Some(""), "app", None, "orders").is_none(), "an empty connection id is no scope");
}

#[test]
fn menu_presence_requires_scope_and_backend() {
    let scope = derive_scope(Some("conn-1"), "app", None, "orders");
    assert!(menu_present(scope.as_ref(), true));
    assert!(!menu_present(scope.as_ref(), false), "a backend without the commands hides the menu");
    assert!(!menu_present(None, true), "no connection id hides the menu");
}

#[test]
fn apply_uses_three_existing_actions_in_order() {
    let plan = apply_plan(&view());
    assert!(!plan.refused);
    assert_eq!(
        plan.actions,
        vec![
            ApplyAction::SetFilters(
                vec![filter("id", "eq", Some(json!(1))), filter("name", "like", Some(json!("a")))],
                FilterLogic::Or,
            ),
            ApplyAction::SetSort { column: "name".to_owned(), descending: true },
            ApplyAction::SetVisibleColumns(vec!["id".to_owned(), "name".to_owned()]),
        ]
    );

    let mut minimal = view();
    minimal.sorts.clear();
    minimal.visible_columns = None;
    let plan = apply_plan(&minimal);
    assert_eq!(plan.actions.len(), 1, "null layout and no sort change nothing else");
    assert!(matches!(plan.actions[0], ApplyAction::SetFilters(_, _)));
}

#[test]
fn raw_where_wins_and_unknown_operators_refuse() {
    let mut raw = view();
    raw.raw_where = Some("id > 5".to_owned());
    let plan = apply_plan(&raw);
    assert_eq!(
        plan.actions[0],
        ApplyAction::ApplyRawWhere("id > 5".to_owned()),
        "a saved raw predicate replays through the raw channel"
    );
    assert!(plan.actions.iter().any(|a| matches!(a, ApplyAction::SetSort { .. })), "sort/columns still follow");
    assert!(!plan.actions.iter().any(|a| matches!(a, ApplyAction::SetFilters(_, _))), "filters are not set on the raw path");

    let mut unknown = view();
    unknown.filters = vec![filter("id", "no-such-operator", Some(json!(1)))];
    let plan = apply_plan(&unknown);
    assert!(plan.refused, "an unknown operator refuses the whole view");
    assert!(plan.actions.is_empty(), "nothing is applied on a refusal");
    let empty_plan = apply_plan(&SavedViewVm::default());
    assert_eq!(empty_plan.actions.len(), 1, "an empty view still sets the (empty) filter list");
    assert!(matches!(empty_plan.actions[0], ApplyAction::SetFilters(_, _)));
}

#[test]
fn dirty_flag_compares_logic_and_filters() {
    let applied = vec![filter("id", "eq", Some(json!(1)))];
    let same = applied.clone();
    let other = vec![filter("id", "eq", Some(json!(2)))];
    assert!(filter_draft_equals_applied(&same, FilterLogic::And, &applied, FilterLogic::And));
    assert!(!filter_draft_equals_applied(&same, FilterLogic::Or, &applied, FilterLogic::And), "logic is part of the comparison");
    assert!(!filter_draft_equals_applied(&other, FilterLogic::And, &applied, FilterLogic::And), "a value change makes it dirty");

    assert!(!draft_is_dirty(&same, FilterLogic::And, &applied, FilterLogic::And));
    assert!(draft_is_dirty(&other, FilterLogic::And, &applied, FilterLogic::And));
}
