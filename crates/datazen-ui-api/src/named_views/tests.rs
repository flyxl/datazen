use std::collections::HashSet;

use serde_json::{json, Value};

use super::*;
use crate::{FilterConnector, FilterOperator, SortCondition};

const NOW: &str = "2026-01-02T03:04:05.000Z";

fn scope(table: &str, database: Option<&str>, schema: Option<&str>) -> NamedViewScope {
    NamedViewScope {
        connection_id: "conn".to_owned(),
        database: database.map(str::to_owned),
        schema: schema.map(str::to_owned),
        table: table.to_owned(),
    }
}

fn named_view(id: &str, name: &str, is_default: bool) -> NamedView {
    NamedView {
        id: id.to_owned(),
        name: name.to_owned(),
        scope: scope("users", Some("db"), None),
        filters: Vec::new(),
        filter_logic: "and".to_owned(),
        raw_where: None,
        sorts: Vec::new(),
        visible_columns: None,
        column_order: None,
        is_default,
        created_at: NOW.to_owned(),
        updated_at: None,
    }
}

fn filter(column: &str, operator: FilterOperator) -> crate::FilterCondition {
    crate::FilterCondition {
        column: column.to_owned(),
        operator,
        value: None,
        connector: None,
    }
}

#[test]
fn export_version_and_availability_gate_are_pinned() {
    assert_eq!(NAMED_VIEW_EXPORT_VERSION, 1);
    assert!(!named_views_available(None));
    assert!(named_views_available(Some(
        &BackendClientCapabilities::default()
    )));
    assert!(!named_views_available(Some(&BackendClientCapabilities {
        list_named_views_unsupported: true
    })));
}

#[test]
fn names_are_trimmed_collapsed_and_compared_case_insensitively() {
    assert_eq!(normalize_view_name("  a   b\t c \n"), "a b c");
    assert_eq!(normalize_view_name("   "), "");
    assert_eq!(normalized_name_key("  Users  "), "users");
    assert!(view_names_equal("Users", " users "));
    assert!(!view_names_equal("Users", "Admins"));
}

#[test]
fn scope_key_tags_null_and_empty_distinctly() {
    let null_scope = scope("t", None, None);
    let empty_scope = scope("t", Some(""), Some(""));
    assert_eq!(named_view_scope_key(&null_scope), "c:conn|d:n:|s:n:|t:t");
    assert_eq!(named_view_scope_key(&empty_scope), "c:conn|d:s:|s:s:|t:t");
    assert_ne!(
        named_view_scope_key(&null_scope),
        named_view_scope_key(&empty_scope)
    );
}

#[test]
fn import_name_suffixes_collisions_case_insensitively() {
    let taken = HashSet::from(["Users".to_owned()]);
    assert_eq!(resolve_import_name("  admins  ", &taken), "admins");
    assert_eq!(resolve_import_name("users", &taken), "users (2)");
    assert_eq!(resolve_import_name("USERS", &taken), "USERS (2)");
    let taken2 = HashSet::from(["Users".to_owned(), "users (2)".to_owned()]);
    assert_eq!(resolve_import_name("Users", &taken2), "Users (3)");
    assert_eq!(resolve_import_name("   ", &taken), "");
}

#[test]
fn scope_filtering_and_sorting_follow_the_host_order() {
    let mut first = named_view("3", "zeta", false);
    first.scope = scope("orders", Some("db"), None);
    let views = vec![
        named_view("1", "beta", false),
        named_view("2", "Alpha", false),
        first,
        named_view("4", "default", true),
    ];
    let scoped = views_for_scope(&views, &scope("users", Some("db"), None));
    assert_eq!(scoped.len(), 3);
    let sorted = sort_named_views(&views);
    let ids: Vec<&str> = sorted.iter().map(|view| view.id.as_str()).collect();
    assert_eq!(
        ids,
        ["4", "2", "1", "3"],
        "sorting does not filter by scope"
    );
    assert_eq!(views[0].id, "1", "the input is not mutated");
}

#[test]
fn view_from_state_uses_applied_facts_and_gaps() {
    let state = TableStateForView {
        scope: scope("users", Some("db"), None),
        filters: vec![filter("id", FilterOperator::Eq)],
        filter_logic: "or".to_owned(),
        raw_where: Some("id > 5".to_owned()),
        sorts: vec![SortCondition {
            column: "id".to_owned(),
            descending: true,
        }],
        visible_columns: Some(vec!["id".to_owned()]),
        column_order: Some(vec!["id".to_owned()]),
    };
    let options = ViewFromTableStateOptions {
        id: "",
        name: "Mine",
        is_default: true,
        now: NOW,
    };
    let view = view_from_table_state(&state, &options);
    assert_eq!(view.id, "");
    assert_eq!(view.name, "Mine");
    assert_eq!(view.filter_logic, "or");
    assert_eq!(view.raw_where.as_deref(), Some("id > 5"));
    assert!(view.is_default);
    assert_eq!(view.created_at, NOW);
    assert_eq!(view.updated_at, None);
    assert_eq!(view.filters.len(), 1);
}

#[test]
fn host_operators_cover_shared_and_dialect_sets() {
    assert!(is_host_filter_operator("eq"));
    assert!(is_host_filter_operator("regex"));
    assert!(is_host_filter_operator("jsonContains"));
    assert!(!is_host_filter_operator("customOp"));
}

#[test]
fn read_view_filters_refuses_unknown_operators_once() {
    let conditions = vec![
        PortFilterCondition {
            column: "a".to_owned(),
            operator: "eq".to_owned(),
            value: Some(json!(1)),
            connector: Some(FilterConnector::And),
        },
        PortFilterCondition {
            column: "b".to_owned(),
            operator: "weird".to_owned(),
            value: None,
            connector: None,
        },
        PortFilterCondition {
            column: "c".to_owned(),
            operator: "weird".to_owned(),
            value: None,
            connector: None,
        },
    ];
    let readout = read_view_filters(&conditions);
    assert_eq!(readout.filters.len(), 1);
    assert_eq!(readout.filters[0].operator, FilterOperator::Eq);
    assert_eq!(readout.filters[0].value, Some(json!(1)));
    assert_eq!(readout.unknown_operators, ["weird"]);
    assert!(read_view_filters(&[]).unknown_operators.is_empty());
}

#[test]
fn export_build_round_trips_a_valid_file() {
    let views = vec![named_view("1", "Mine", false)];
    let file = build_named_view_export(&views, NOW);
    assert_eq!(file.version, 1);
    assert_eq!(file.exported_at, NOW);
    let text = json!({
        "version": 1,
        "exportedAt": NOW,
        "views": [{
            "id": "1", "name": "Mine",
            "scope": {"connectionId": "conn", "database": "db", "schema": null, "table": "users"},
            "filters": [], "filterLogic": "and", "sorts": []
        }]
    })
    .to_string();
    let parsed = parse_named_view_export(&text, NOW).unwrap();
    assert_eq!(parsed.version, 1);
    assert_eq!(parsed.exported_at, NOW);
    assert_eq!(parsed.views.len(), 1);
    assert_eq!(parsed.views[0].name, "Mine");
}

#[test]
fn export_parsing_refuses_bad_files_with_readable_reasons() {
    assert_eq!(
        parse_named_view_export("not json", NOW).unwrap_err(),
        "The file is not valid JSON."
    );
    assert_eq!(
        parse_named_view_export("[]", NOW).unwrap_err(),
        "The file does not hold a saved-view export."
    );
    assert_eq!(
        parse_named_view_export(r#"{"version":2,"views":[]}"#, NOW).unwrap_err(),
        "The file has version 2, expected 1."
    );
    assert_eq!(
        parse_named_view_export(r#"{"version":1,"views":{}}"#, NOW).unwrap_err(),
        "The file does not hold a list of views."
    );
    assert_eq!(
        parse_named_view_export(r#"{"version":1,"views":[42]}"#, NOW).unwrap_err(),
        "The file holds a view that is not a saved view."
    );
}

#[test]
fn exported_view_defaults_missing_optional_fields() {
    let text = r#"{
        "version": 1,
        "views": [{
            "id": "v1", "name": "Mine",
            "scope": {"connectionId": "c", "table": "t"},
            "filters": [], "filterLogic": "and", "sorts": []
        }]
    }"#;
    let parsed = parse_named_view_export(text, NOW).unwrap();
    let view = &parsed.views[0];
    assert_eq!(view.scope.database, None);
    assert_eq!(view.scope.schema, None);
    assert_eq!(view.raw_where, None);
    assert_eq!(view.visible_columns, None);
    assert_eq!(view.column_order, None);
    assert!(!view.is_default);
    assert_eq!(view.created_at, NOW);
    assert_eq!(view.updated_at, None);
    assert_eq!(parsed.exported_at, NOW);
}

#[test]
fn exported_view_rejects_bad_filter_logic_and_scopes() {
    let bad_logic = r#"{
        "version": 1,
        "views": [{"id":"v","name":"n","scope":{"connectionId":"c","table":"t"},
                   "filters":[],"filterLogic":"xor","sorts":[]}]
    }"#;
    assert_eq!(
        parse_named_view_export(bad_logic, NOW).unwrap_err(),
        "The file holds a view that is not a saved view."
    );
    let bad_scope = r#"{
        "version": 1,
        "views": [{"id":"v","name":"n","scope":{"table":"t"},
                   "filters":[],"filterLogic":"and","sorts":[]}]
    }"#;
    assert_eq!(
        parse_named_view_export(bad_scope, NOW).unwrap_err(),
        "The file holds a view that is not a saved view."
    );
    let _: Value = json!({});
}
