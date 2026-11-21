use std::collections::BTreeMap;

use serde_json::json;

use crate::{
    types::ColumnSchema, DefaultOrderOverride, FilterCondition, FilterLogic, FilterMode,
    FilterOperator, SortCondition, TableState,
};

use super::*;

fn snapshot(panel: &str, context: Option<serde_json::Value>) -> TableDataStoreSnapshot {
    let mut state = TableState::empty();
    state.context = context;
    let mut by_panel = BTreeMap::new();
    by_panel.insert(panel.to_owned(), state);
    TableDataStoreSnapshot { by_panel }
}

fn state_at<'a>(store: &'a TableDataStoreSnapshot, panel: &str) -> &'a TableState {
    &store.by_panel[panel]
}

fn condition(column: &str, value: &str) -> FilterCondition {
    FilterCondition {
        column: column.to_owned(),
        operator: FilterOperator::Eq,
        value: Some(json!(value)),
        connector: None,
    }
}

fn sort(column: &str, descending: bool) -> SortCondition {
    SortCondition { column: column.to_owned(), descending }
}

#[test]
fn begin_load_deduplicates_an_inflight_request_and_resolves_page_size() {
    let store = snapshot("p1", None);
    let context = TableContextData { db_session_id: "s1".to_owned(), table: "t".to_owned(), ..Default::default() };
    let started = begin_load(state_at(&store, "p1"), &context, 100, Some(25), vec![], None).expect("a fresh load");
    assert_eq!(started.page_size, 100, "the setting beats the driver default");
    assert!(started.loading);
    assert_eq!(started.loading_revision, Some(started.request_revision));
    assert!(started.context.is_some());

    let driver_only = begin_load(state_at(&store, "p1"), &context, 0, Some(25), vec![], None).expect("a load");
    assert_eq!(driver_only.page_size, 25);
    let no_size = begin_load(state_at(&store, "p1"), &context, 0, None, vec![], None).expect("a load");
    assert_eq!(no_size.page_size, 0, "an empty panel keeps its page size");

    let mut state = TableState::empty();
    state.loading = true;
    state.loading_revision = Some(0);
    assert!(begin_load(&state, &context, 100, None, vec![], None).is_none(), "a duplicate in-flight load is a no-op");
}

#[test]
fn a_failed_load_requires_the_matching_revision_and_clears_selection() {
    let mut state = TableState::empty();
    state.request_revision = 3;
    state.loading_revision = Some(3);
    state.selected_rows.insert(1);
    assert!(load_failed(&state, 2, "x").is_none());
    let failed = load_failed(&state, 3, "boom").expect("a current failure");
    assert_eq!(failed.error.as_deref(), Some("boom"));
    assert!(!failed.loading);
    assert_eq!(failed.loading_revision, None);
    assert!(failed.selected_rows.is_empty());
}

#[test]
fn paging_actions_return_a_reload_request_and_reset_the_page() {
    let store = snapshot("p1", None);
    let (paged, reload) = set_page(&store, "p1", 4);
    assert_eq!(state_at(&paged, "p1").page, 4);
    assert_eq!(reload.and_then(|r| r.skip_count), Some(true));

    let (sized, reload) = set_page_size(&store, "p1", 25);
    assert_eq!(state_at(&sized, "p1").page_size, 25);
    assert_eq!(state_at(&sized, "p1").page, 0);
    assert_eq!(reload.and_then(|r| r.skip_count), Some(true));

    let (missing, reload) = set_page(&store, "missing", 1);
    assert_eq!(missing, store);
    assert!(reload.is_some(), "the caller still gets a reload decision for an unknown panel");
}

#[test]
fn filter_actions_separate_render_only_edits_from_reload_transitions() {
    let store = snapshot("p1", None);
    let added = add_filter(&store, "p1", condition("a", "x"));
    assert_eq!(state_at(&added, "p1").draft_filters.len(), 1);
    assert!(state_at(&added, "p1").filter_panel_open);
    assert_eq!(state_at(&added, "p1").request_revision, 0, "a draft edit is render-only");

    let updated = update_filter(&added, "p1", 0, condition("a", "y"));
    assert_eq!(state_at(&updated, "p1").draft_filters[0].value, Some(json!("y")));
    let removed = remove_filter(&updated, "p1", 0);
    assert!(state_at(&removed, "p1").draft_filters.is_empty());
    let logic = set_filter_logic(&store, "p1", FilterLogic::Or);
    assert_eq!(state_at(&logic, "p1").draft_filter_logic, FilterLogic::Or);
    let mode = set_filter_mode(&store, "p1", FilterMode::RawSql);
    assert_eq!(state_at(&mode, "p1").filter_mode, FilterMode::RawSql);

    let (applied, reload) = set_filters(&store, "p1", vec![condition("a", "x")], FilterLogic::Or);
    assert_eq!(state_at(&applied, "p1").filters.len(), 1);
    assert_eq!(state_at(&applied, "p1").filter_logic, FilterLogic::Or);
    assert_eq!(state_at(&applied, "p1").request_revision, 1);
    assert!(reload.is_some());
}

#[test]
fn applying_filters_refuses_incomplete_or_unchanged_drafts() {
    let store = snapshot("p1", None);
    let (unchanged, reload) = apply_filters(&store, "p1");
    assert_eq!(unchanged, store);
    assert!(reload.is_none());

    let with_draft = add_filter(&store, "p1", condition("a", "x"));
    let (applied, reload) = apply_filters(&with_draft, "p1");
    assert_eq!(state_at(&applied, "p1").filters.len(), 1);
    assert_eq!(state_at(&applied, "p1").request_revision, 1);
    assert!(reload.is_some());

    let mut incomplete = state_at(&store, "p1").clone();
    incomplete.draft_filters = vec![FilterCondition {
        column: "a".to_owned(),
        operator: FilterOperator::Eq,
        value: Some(json!("")),
        connector: None,
    }];
    let mut incomplete_store = TableDataStoreSnapshot::default();
    incomplete_store.by_panel.insert("p1".to_owned(), incomplete);
    let (refused, reload) = apply_filters(&incomplete_store, "p1");
    assert_eq!(refused, incomplete_store);
    assert!(reload.is_none());
}

#[test]
fn raw_and_any_column_channels_replace_each_other_and_empty_means_clear() {
    let store = snapshot("p1", None);
    let (raw, _) = apply_raw_where(&store, "p1", "  x = 1  ");
    assert_eq!(state_at(&raw, "p1").raw_where.as_deref(), Some("x = 1"));
    assert!(state_at(&raw, "p1").filters.is_empty());

    let (any, _) = apply_any_column(&raw, "p1", &AnyColumnExpansion { conditions: vec![condition("a", "q")] });
    assert_eq!(state_at(&any, "p1").filter_logic, FilterLogic::Or);
    assert_eq!(state_at(&any, "p1").raw_where, None);
    assert_eq!(state_at(&any, "p1").filters.len(), 1);

    let empty = AnyColumnExpansion { conditions: vec![] };
    let (cleared, _) = apply_any_column(&any, "p1", &empty);
    assert!(state_at(&cleared, "p1").filters.is_empty());
    assert_eq!(state_at(&cleared, "p1").filter_mode, FilterMode::Structured);

    let (cleared_raw, _) = apply_raw_where(&any, "p1", "   ");
    assert_eq!(state_at(&cleared_raw, "p1").raw_where, None);
    let (cleared_filters, _) = clear_filters(&any, "p1");
    assert_eq!(state_at(&cleared_filters, "p1").draft_filter_logic, FilterLogic::And);
}

#[test]
fn sort_actions_persist_a_revision_and_preserve_prior_columns() {
    let store = snapshot("p1", None);
    let (first, _) = add_sort(&store, "p1", sort("a", false));
    let (second, _) = add_sort(&first, "p1", sort("b", true));
    assert_eq!(state_at(&second, "p1").sorts, vec![sort("a", false), sort("b", true)]);
    let (reaimed, _) = add_sort(&second, "p1", sort("a", true));
    assert_eq!(state_at(&reaimed, "p1").sorts, vec![sort("a", true), sort("b", true)]);
    let (removed, _) = remove_sort(&reaimed, "p1", "a");
    assert_eq!(state_at(&removed, "p1").sorts, vec![sort("b", true)]);
    let (cleared, _) = clear_sorts(&removed, "p1");
    assert!(state_at(&cleared, "p1").sorts.is_empty());
    assert_eq!(state_at(&cleared, "p1").request_revision, 5);

    let (replaced, _) = set_sort(&second, "p1", sort("c", false));
    assert_eq!(state_at(&replaced, "p1").sorts, vec![sort("c", false)]);
    let (overridden, _) = set_default_order_override(&second, "p1", DefaultOrderOverride::PrimaryKeyDesc);
    assert_eq!(state_at(&overridden, "p1").default_order_override, DefaultOrderOverride::PrimaryKeyDesc);
    assert!(state_at(&overridden, "p1").sorts.is_empty());
}

#[test]
fn render_only_actions_never_bump_the_revision() {
    let store = snapshot("p1", None);
    let ordered = set_column_order(&store, "p1", Some(vec!["b".to_owned()]));
    assert_eq!(state_at(&ordered, "p1").column_order.as_deref(), Some(&["b".to_owned()][..]));
    assert_eq!(state_at(&ordered, "p1").request_revision, 0);
    let frozen = set_frozen_column_count(&store, "p1", 9);
    assert_eq!(state_at(&frozen, "p1").frozen_column_count, 9);
    let detail = set_detail_row(&store, "p1", Some(3));
    assert_eq!(state_at(&detail, "p1").detail_row_index, Some(3));
    let visible = set_visible_columns(&store, "p1", Some(vec!["a".to_owned()]));
    assert_eq!(state_at(&visible, "p1").visible_columns.as_deref(), Some(&["a".to_owned()][..]));
    let editing = start_edit(&store, "p1", 2, "name".to_owned());
    assert_eq!(state_at(&editing, "p1").editing_cell.as_ref().map(|c| c.row), Some(2));
    assert!(state_at(&cancel_edit(&editing, "p1"), "p1").editing_cell.is_none());
    assert_eq!(state_at(&editing, "p1").request_revision, 0);
}

#[test]
fn invalidation_skips_staged_edits_open_editors_and_empty_panels() {
    let context = json!({"dbSessionId":"s1","table":"t"});
    let store = snapshot("p1", Some(context.clone()));
    let mut state = state_at(&store, "p1").clone();
    state.columns = vec![ColumnSchema {
        name: "id".to_owned(),
        data_type: "int".to_owned(),
        nullable: false,
        default_value: None,
        is_primary_key: Some(true),
        is_auto_increment: None,
        comment: None,
    }];
    state.rows = vec![BTreeMap::from([("id".to_owned(), json!(1))])];
    let mut full = TableDataStoreSnapshot::default();
    full.by_panel.insert("p1".to_owned(), state);

    let invalidated = invalidate_cached_data(&full, "s1", None);
    assert!(state_at(&invalidated, "p1").rows.is_empty());
    assert!(state_at(&invalidated, "p1").columns.is_empty());
    assert_eq!(state_at(&invalidated, "p1").request_revision, 1);
    assert_eq!(invalidate_cached_data(&full, "other", None), full, "a different session is untouched");
    assert_eq!(invalidate_cached_data(&full, "s1", Some("other")), full, "a different table is untouched");

    let mut open = state_at(&full, "p1").clone();
    open.editing_cell = Some(crate::CellEditLocation { row: 0, col: "id".to_owned() });
    let mut open_store = TableDataStoreSnapshot::default();
    open_store.by_panel.insert("p1".to_owned(), open);
    assert_eq!(invalidate_cached_data(&open_store, "s1", None), open_store, "an open editor keeps its rows");

    let mut staged = state_at(&full, "p1").clone();
    staged.pending_changes.insert(
        "k".to_owned(),
        crate::PendingRowChange {
            row_index: None,
            row_identity: BTreeMap::new(),
            original_values: BTreeMap::new(),
            current_values: BTreeMap::new(),
            changed_columns: vec![],
            delete_marked: false,
            insert_marked: None,
            expression_values: None,
        },
    );
    let mut staged_store = TableDataStoreSnapshot::default();
    staged_store.by_panel.insert("p1".to_owned(), staged);
    assert_eq!(invalidate_cached_data(&staged_store, "s1", None), staged_store, "staged work is never dropped");
}

#[test]
fn pending_status_and_clear_actions_are_render_only() {
    let mut state = TableState::empty();
    state.editing_cell = Some(crate::CellEditLocation { row: 0, col: "a".to_owned() });
    state.highlighted_new_row_key = Some("k".to_owned());
    let mut store = TableDataStoreSnapshot::default();
    store.by_panel.insert("p1".to_owned(), state);
    let previewing = set_pending_status(&store, "p1", PendingStatus::Previewing);
    assert_eq!(state_at(&previewing, "p1").pending_status, json!("previewing"));
    let cleared = clear_pending_state(&previewing, "p1");
    assert!(state_at(&cleared, "p1").editing_cell.is_none());
    assert_eq!(state_at(&cleared, "p1").highlighted_new_row_key, None);
    assert_eq!(state_at(&cleared, "p1").pending_status, json!("idle"));
    assert_eq!(state_at(&cleared, "p1").request_revision, 0);
    assert!(clear_highlighted_new_row(&store, "p1").by_panel["p1"].highlighted_new_row_key.is_none());
}

#[test]
fn crate_root_action_surface_is_reexported() {
    let _ = crate::begin_load;
    let _ = crate::load_failed;
    let _ = crate::set_page;
    let _ = crate::set_page_size;
    let _ = crate::add_filter;
    let _ = crate::apply_filters;
    let _ = crate::apply_raw_where;
    let _ = crate::add_sort;
    let _ = crate::set_column_order;
    let _ = crate::invalidate_cached_data;
    let _ = crate::clear_pending_state;
    let _ = crate::resolve_default_table_sort;
    let _ = crate::target_of;
    let _ = crate::duplicate_identity_keys;
    let _: Option<crate::PendingStatus> = None;
    let _: Option<crate::TableContextData> = None;
    let _: Option<crate::PageCommit> = None;
}
