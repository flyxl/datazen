use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::{
    types::ColumnSchema, DefaultOrderOverride, DefaultTableSort, FilterCondition, FilterLogic,
    FilterMode, FilterOperator, SortCondition, TableState,
};

use super::*;

fn column(name: &str, primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: "int".to_owned(),
        nullable: false,
        default_value: None,
        is_primary_key: Some(primary_key),
        is_auto_increment: None,
        comment: None,
    }
}

fn sort(column_name: &str, descending: bool) -> SortCondition {
    SortCondition { column: column_name.to_owned(), descending }
}

fn snapshot(panel: &str) -> TableDataStoreSnapshot {
    let mut by_panel = BTreeMap::new();
    by_panel.insert(panel.to_owned(), TableState::empty());
    TableDataStoreSnapshot { by_panel }
}

#[test]
fn surface_and_gap_are_pinned() {
    assert_eq!(TABLE_DATA_STORE_SURFACE.len(), 9);
    assert!(TABLE_DATA_STORE_SURFACE.contains(&"orderRequest"));
    assert_eq!(TABLE_DATA_STORE_GAP.len(), 8);
    assert_eq!(crate::TABLE_DATA_STORE_SURFACE.len(), 9);
}

#[test]
fn default_order_resolution_prefers_the_override() {
    assert_eq!(
        resolve_default_table_sort(DefaultOrderOverride::Inherit, DefaultTableSort::None),
        DefaultTableSort::None
    );
    assert_eq!(
        resolve_default_table_sort(DefaultOrderOverride::PrimaryKeyAsc, DefaultTableSort::None),
        DefaultTableSort::PrimaryKeyAsc
    );
    assert_eq!(DefaultTableSort::PrimaryKeyDesc.as_str(), "primaryKeyDesc");
}

#[test]
fn order_request_handles_none_explicit_sorts_and_single_key_desc() {
    let columns = vec![column("id", true), column("name", false)];
    let sorts = vec![sort("name", false)];
    let none = order_request(
        DefaultOrderOverride::Inherit,
        &columns,
        &sorts,
        DefaultTableSort::None,
    );
    assert_eq!(none.default_order_columns, Some(Vec::new()));
    assert_eq!(none.sorts, sorts);

    let explicit_wins = order_request(
        DefaultOrderOverride::PrimaryKeyDesc,
        &columns,
        &sorts,
        DefaultTableSort::PrimaryKeyAsc,
    );
    assert_eq!(explicit_wins.sorts, sorts, "an explicit sort beats the descending override");

    let key_desc = order_request(
        DefaultOrderOverride::PrimaryKeyDesc,
        &columns,
        &[],
        DefaultTableSort::PrimaryKeyAsc,
    );
    assert_eq!(key_desc.sorts, vec![sort("id", true)]);
    assert_eq!(key_desc.default_order_columns, None);

    let composite = vec![column("a", true), column("b", true)];
    let composite_desc = order_request(
        DefaultOrderOverride::PrimaryKeyDesc,
        &composite,
        &[],
        DefaultTableSort::PrimaryKeyAsc,
    );
    assert!(composite_desc.sorts.is_empty(), "a composite key cannot use the one-column channel");
}

#[test]
fn upsert_sort_keeps_position_and_appends_new_columns() {
    let sorts = vec![sort("a", false), sort("b", false)];
    assert_eq!(upsert_sort(&sorts, sort("a", true)), vec![sort("a", true), sort("b", false)]);
    assert_eq!(
        upsert_sort(&sorts, sort("c", true)),
        vec![sort("a", false), sort("b", false), sort("c", true)]
    );
}

#[test]
fn target_of_copies_every_context_dimension() {
    let context = TableContextData {
        db_session_id: "s1".to_owned(),
        table: "users".to_owned(),
        connection_id: Some("c1".to_owned()),
        driver_type: Some("postgres".to_owned()),
        database: Some("db".to_owned()),
        schema: Some("public".to_owned()),
    };
    let target = target_of(&context);
    assert_eq!(target.db_session_id, "s1");
    assert_eq!(target.table, "users");
    assert_eq!(target.connection_id.as_deref(), Some("c1"));
    assert_eq!(target.driver_type.as_deref(), Some("postgres"));
    assert_eq!(target.database.as_deref(), Some("db"));
    assert_eq!(target.schema.as_deref(), Some("public"));
}

#[test]
fn panel_patches_are_noops_for_unknown_panels_and_bump_revision_on_reload() {
    let store = snapshot("p1");
    assert_eq!(patch_panel(&store, "missing", |state| state.page = 5), store);
    assert_eq!(patch_panel_for_reload(&store, "missing", |_| {}), store);

    let mut state = TableState::empty();
    state.request_revision = 3;
    state.highlighted_new_row_key = Some("k".to_owned());
    state.selected_rows.insert(0);
    let mut one = TableDataStoreSnapshot::default();
    one.by_panel.insert("p1".to_owned(), state);
    let patched = patch_panel_for_reload(&one, "p1", |state| state.page = 2);
    let entry = &patched.by_panel["p1"];
    assert_eq!(entry.page, 2);
    assert_eq!(entry.request_revision, 4);
    assert_eq!(entry.highlighted_new_row_key, None);
    assert!(entry.selected_rows.is_empty());
    assert_eq!(one.by_panel["p1"].request_revision, 3, "the original is untouched");
}

#[test]
fn rows_records_duplicate_detection_and_pending_overlay() {
    let columns = vec![column("id", true), column("name", false)];
    let rows = vec![vec![json!(1), json!("a")], vec![json!(2), json!("b")]];
    let records = rows_to_records(&columns, &rows);
    assert_eq!(records[0]["name"], json!("a"));
    let pk = ["id".to_owned()];
    assert!(duplicate_identity_keys(&records, &pk).is_empty());
    let duplicates = duplicate_identity_keys(
        &rows_to_records(&columns, &[vec![json!(3), json!("x")], vec![json!(3), json!("y")]]),
        &pk,
    );
    assert_eq!(duplicates.len(), 1);
    assert!(duplicates.contains("[\"id\":3]"));

    let change = crate::PendingRowChange {
        row_index: Some(0),
        row_identity: BTreeMap::from([("id".to_owned(), json!(1))]),
        original_values: BTreeMap::new(),
        current_values: BTreeMap::from([("name".to_owned(), json!("new"))]),
        changed_columns: vec!["name".to_owned()],
        delete_marked: false,
        insert_marked: None,
        expression_values: None,
    };
    let mut pending = BTreeMap::new();
    pending.insert("[\"id\":1]".to_owned(), change);
    let mut anchors = BTreeMap::new();
    anchors.insert(0_u64, "[\"id\":1]".to_owned());
    let overlaid = overlay_pending_rows(&records, &anchors, &pending, &pk);
    assert_eq!(overlaid[0]["name"], json!("new"));
    assert_eq!(overlaid[1]["name"], json!("b"), "the other row is untouched");

    let ambiguous = vec![vec![json!(4), json!("m")], vec![json!(4), json!("n")]];
    let ambiguous_records = rows_to_records(&columns, &ambiguous);
    let mut ambiguous_pending = BTreeMap::new();
    ambiguous_pending.insert("[\"id\":4]".to_owned(), crate::PendingRowChange {
        row_index: Some(0),
        row_identity: BTreeMap::from([("id".to_owned(), json!(4))]),
        original_values: BTreeMap::new(),
        current_values: BTreeMap::from([("name".to_owned(), json!("new"))]),
        changed_columns: vec!["name".to_owned()],
        delete_marked: false,
        insert_marked: None,
        expression_values: None,
    });
    let unchanged = overlay_pending_rows(&ambiguous_records, &BTreeMap::new(), &ambiguous_pending, &pk);
    assert_eq!(unchanged[0]["name"], json!("m"), "an ambiguous identity is not overlaid");
    assert_eq!(unchanged[1]["name"], json!("n"));
}

#[test]
fn visibility_sanitization_keeps_surviving_names_and_drops_empty_orders() {
    let valid = BTreeSet::from(["a".to_owned(), "b".to_owned()]);
    let (visible, order) = sanitize_visibility(
        Some(&vec!["a".to_owned(), "ghost".to_owned()]),
        Some(&vec!["ghost".to_owned(), "b".to_owned()]),
        &valid,
        2,
    );
    assert_eq!(visible, Some(vec!["a".to_owned()]), "a partial visible list stays partial");
    assert_eq!(order, Some(vec!["b".to_owned()]));
    let (full, _) = sanitize_visibility(
        Some(&vec!["a".to_owned(), "b".to_owned()]),
        None,
        &valid,
        2,
    );
    assert_eq!(full, None, "a full visible list collapses back to null");
    let (_, empty_order) = sanitize_visibility(None, Some(&vec!["ghost".to_owned()]), &valid, 2);
    assert_eq!(empty_order, None);
}

fn fetched(columns: Vec<ColumnSchema>, rows: Vec<Vec<Value>>) -> FetchedPage {
    FetchedPage { columns, rows, page: 1, page_size: 50, ..Default::default() }
}

#[test]
fn a_fetched_page_is_discarded_when_superseded() {
    let mut state = TableState::empty();
    state.request_revision = 4;
    let context = json!({"dbSessionId":"s1"});
    assert!(commit_fetched_page(&state, 3, context.clone(), &fetched(vec![], vec![])).is_none());
    assert!(commit_fetched_page(&state, 4, context, &fetched(vec![], vec![])).is_none(), "no in-flight revision");
}

#[test]
fn a_committed_page_updates_rows_count_anchors_and_selection() {
    let mut state = TableState::empty();
    state.request_revision = 2;
    state.loading = true;
    state.loading_revision = Some(2);
    state.selected_rows.insert(0);
    state.visible_columns = Some(vec!["ghost".to_owned()]);
    state.column_order = Some(vec!["ghost".to_owned()]);
    let columns = vec![column("id", true), column("name", false)];
    let page = fetched(columns.clone(), vec![vec![json!(7), json!("n")]]);
    let commit = commit_fetched_page(&state, 2, json!({"dbSessionId":"s1"}), &page).expect("a current page");
    assert_eq!(commit.state.rows.len(), 1);
    assert_eq!(commit.state.page, 1);
    assert_eq!(commit.state.page_size, 50);
    assert!(!commit.state.loading);
    assert_eq!(commit.state.loading_revision, None);
    assert!(commit.state.selected_rows.is_empty());
    assert_eq!(commit.state.visible_columns, Some(Vec::new()));
    assert_eq!(commit.state.column_order, None);
    assert!(commit.state.row_identity_anchors.contains_key(&0));
    assert!(!commit.drift_recount);
}

#[test]
fn a_drift_page_sets_the_recount_marker_once() {
    let mut state = TableState::empty();
    state.request_revision = 1;
    state.loading_revision = Some(1);
    let mut page = fetched(vec![column("id", true)], vec![]);
    page.drift_suspected = true;
    let commit = commit_fetched_page(&state, 1, json!({}), &page).expect("a current page");
    assert_eq!(commit.state.drift_recount_page, Some(1));
    assert!(commit.drift_recount);
    let mut again = commit.state.clone();
    again.request_revision = 2;
    again.loading_revision = Some(2);
    let repeat = commit_fetched_page(&again, 2, json!({}), &page).expect("a current page");
    assert!(!repeat.drift_recount, "one recount per page");
}

#[test]
fn incomplete_filters_are_refused_and_equals_applied_short_circuits() {
    let mut state = TableState::empty();
    state.draft_filters = vec![FilterCondition {
        column: "a".to_owned(),
        operator: FilterOperator::Eq,
        value: Some(json!("")),
        connector: None,
    }];
    assert!(!is_complete_filter(&state.draft_filters[0]));
    assert!(!apply_filters_decision(&state));

    state.draft_filters[0].value = Some(json!("x"));
    assert!(is_complete_filter(&state.draft_filters[0]));
    assert!(apply_filters_decision(&state));
    state.filters = state.draft_filters.clone();
    assert!(filter_draft_equals_applied(&state));
    assert!(!apply_filters_decision(&state));

    let null_filter = FilterCondition {
        column: "a".to_owned(),
        operator: FilterOperator::IsNull,
        value: None,
        connector: None,
    };
    assert!(is_complete_filter(&null_filter), "literal-less operators need no value");
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::order_request;
    let _ = crate::upsert_sort;
    let _ = crate::commit_fetched_page;
    let _ = crate::rows_to_records;
    let _ = crate::sanitize_visibility;
    let _ = crate::is_complete_filter;
    let _ = crate::apply_filters_decision;
    let _ = crate::patch_panel
        as fn(&crate::TableDataStoreSnapshot, &str, fn(&mut crate::TableState)) -> crate::TableDataStoreSnapshot;
    let _ = crate::patch_panel_for_reload
        as fn(&crate::TableDataStoreSnapshot, &str, fn(&mut crate::TableState)) -> crate::TableDataStoreSnapshot;
    let _ = crate::TABLE_DATA_STORE_SURFACE;
    let _: Option<crate::TableDataStoreSnapshot> = None;
    let _: Option<crate::FetchedPage> = None;
    let _: Option<crate::LoadTableDataParams> = None;
    let _: Option<crate::ReloadRequest> = None;
    let _: Option<crate::DefaultTableSort> = None;
}
