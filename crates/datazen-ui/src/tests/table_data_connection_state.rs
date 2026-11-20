//! T041: React `stores/tableData/connectionState.ts` parity. Every assertion
//! below pins one behavior of the port — the record shape (`row[i] ?? null`,
//! surplus row values dropped), the null collapse of `toCellValue`, the
//! branch order and blank rejection of `extractErrorMessage`, every default
//! of `emptyTableState` (including the shared-constant copies), and the
//! `?? null` mapping of `buildTableContext`.

use std::error::Error;
use std::fmt;

use serde_json::{Value, json};

use crate::table_data_connection_state::{
    ColumnSchema, CountStrategy, DefaultOrderOverride, EMPTY_GRID_SELECTION,
    EMPTY_KEYSET_PAGING_STATE, ErrorMessageSource, FilterLogic, FilterMode, GridSelectionMode,
    PagingMode, PendingStatus, TableChangeContext, TableState, TableTarget, build_table_context,
    empty_table_state, extract_error_message, rows_to_records, to_cell_value,
};

fn col(name: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: "text".to_owned(),
        nullable: true,
        default_value: None,
        is_primary_key: false,
        is_auto_increment: false,
        comment: None,
    }
}

fn context() -> TableChangeContext {
    TableChangeContext {
        connection_id: Some("conn-1".to_owned()),
        db_session_id: "sess-9".to_owned(),
        driver_type: Some("postgres".to_owned()),
        database: Some("app_db".to_owned()),
        schema: Some("public".to_owned()),
        table: "users".to_owned(),
    }
}

/// A test error whose `Display` is the message it carries.
#[derive(Debug)]
struct MsgError(&'static str);

impl fmt::Display for MsgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Error for MsgError {}

#[test]
fn rows_to_records_maps_columns_by_position() {
    let columns = [col("id"), col("name"), col("active")];
    let rows = vec![
        vec![json!(7), json!("ada"), json!(true)],
        vec![json!(8), json!("grace"), json!(false)],
    ];
    let records = rows_to_records(&columns, &rows);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["id"], json!(7));
    assert_eq!(records[0]["name"], json!("ada"));
    assert_eq!(records[0]["active"], json!(true));
    assert_eq!(records[1]["id"], json!(8));
    assert_eq!(records[1]["name"], json!("grace"));
    assert_eq!(records[1]["active"], json!(false));
}

#[test]
fn rows_to_records_missing_cells_become_null() {
    // Columns longer than the row: the missing cells are null, not absent.
    let columns = [col("id"), col("name"), col("age")];
    let rows = vec![vec![json!(7)], vec![]];
    let records = rows_to_records(&columns, &rows);
    assert_eq!(records[0]["id"], json!(7));
    assert_eq!(records[0]["name"], Value::Null);
    assert_eq!(records[0]["age"], Value::Null);
    assert_eq!(records[1]["id"], Value::Null);
    assert_eq!(records[1]["name"], Value::Null);
    assert_eq!(records[1]["age"], Value::Null);
}

#[test]
fn rows_to_records_extra_row_values_are_dropped() {
    // Row longer than the columns: the surplus values never reach a record.
    let columns = [col("id")];
    let rows = vec![vec![json!(7), json!("surplus"), json!(true)]];
    let records = rows_to_records(&columns, &rows);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].len(), 1);
    assert_eq!(records[0]["id"], json!(7));
}

#[test]
fn rows_to_records_empty_inputs() {
    assert!(rows_to_records(&[col("id")], &[]).is_empty());
    let records = rows_to_records(&[], &[vec![json!(1), json!(2)]]);
    assert_eq!(records.len(), 1);
    assert!(records[0].is_empty());
}

#[test]
fn rows_to_records_keeps_nested_values() {
    let columns = [col("meta"), col("tags")];
    let rows = vec![vec![
        json!({"role": "admin", "level": 3}),
        json!(["a", "b"]),
    ]];
    let records = rows_to_records(&columns, &rows);
    assert_eq!(records[0]["meta"], json!({"role": "admin", "level": 3}));
    assert_eq!(records[0]["tags"], json!(["a", "b"]));
}

#[test]
fn to_cell_value_none_is_null() {
    assert_eq!(to_cell_value(None), Value::Null);
}

#[test]
fn to_cell_value_some_passes_through() {
    assert_eq!(to_cell_value(Some(json!("x"))), json!("x"));
    assert_eq!(to_cell_value(Some(json!(42))), json!(42));
    assert_eq!(to_cell_value(Some(json!(true))), json!(true));
    assert_eq!(to_cell_value(Some(json!([1, 2]))), json!([1, 2]));
    assert_eq!(to_cell_value(Some(json!({"a": 1}))), json!({"a": 1}));
    assert_eq!(to_cell_value(Some(Value::Null)), Value::Null);
}

#[test]
fn extract_error_message_text_wins_verbatim() {
    // A non-blank string is returned as-is — surrounding whitespace included.
    // Only the blank check looks at the trimmed form.
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::Text("boom")), "fallback"),
        "boom"
    );
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::Text("  boom  ")), "fallback"),
        "  boom  "
    );
}

#[test]
fn extract_error_message_blank_text_falls_back() {
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::Text("")), "fallback"),
        "fallback"
    );
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::Text("   \t\n ")), "fallback"),
        "fallback"
    );
}

#[test]
fn extract_error_message_error_message() {
    let error = MsgError("connection refused");
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::Error(&error)), "fallback"),
        "connection refused"
    );
}

#[test]
fn extract_error_message_blank_error_message_falls_back() {
    let error = MsgError("   ");
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::Error(&error)), "fallback"),
        "fallback"
    );
    let empty = MsgError("");
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::Error(&empty)), "fallback"),
        "fallback"
    );
}

#[test]
fn extract_error_message_message_field() {
    assert_eq!(
        extract_error_message(
            Some(ErrorMessageSource::MessageField("object said so")),
            "fallback"
        ),
        "object said so"
    );
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::MessageField("  ")), "fallback"),
        "fallback"
    );
}

#[test]
fn extract_error_message_none_falls_back() {
    assert_eq!(extract_error_message(None, "fallback"), "fallback");
}

#[test]
fn empty_table_state_defaults() {
    let state = empty_table_state(None);
    assert_eq!(state.context, None);
    assert!(state.columns.is_empty());
    assert!(state.rows.is_empty());
    assert_eq!(state.total_rows, 0);
    // A panel that has never read a row has no count to doubt yet.
    assert_eq!(state.total_rows_kind, CountStrategy::Exact);
    assert_eq!(state.default_order_override, DefaultOrderOverride::Inherit);
    assert_eq!(state.page, 0);
    assert_eq!(state.page_size, 50);
    // A panel that has never paged by seek reads by offset.
    assert_eq!(state.keyset, EMPTY_KEYSET_PAGING_STATE);
    assert_eq!(state.keyset.mode, PagingMode::Offset);
    assert!(state.keyset.order_columns.is_empty());
    assert!(state.keyset.stack.is_empty());
    assert_eq!(state.keyset.anchor_page, 0);
    assert_eq!(state.keyset.unavailable, None);
    assert!(!state.keyset.drift_suspected);
    assert_eq!(state.keyset.next_after_key, None);
    assert_eq!(state.drift_recount_page, None);
    assert!(state.filters.is_empty());
    assert_eq!(state.filter_logic, FilterLogic::And);
    assert!(state.draft_filters.is_empty());
    assert_eq!(state.draft_filter_logic, FilterLogic::And);
    assert!(!state.filter_panel_open);
    // DBR-04: the panel starts in the structured editor, with both new-mode
    // drafts empty and the raw channel idle.
    assert_eq!(state.filter_mode, FilterMode::Structured);
    assert_eq!(state.any_column_draft, "");
    assert_eq!(state.raw_where_draft, "");
    assert_eq!(state.raw_where, None);
    assert!(state.sorts.is_empty());
    assert_eq!(state.column_order, None);
    assert_eq!(state.frozen_column_count, 0);
    assert!(state.pending_changes.is_empty());
    assert!(state.draft_inserts.is_empty());
    assert!(state.draft_insert_order.is_empty());
    assert_eq!(state.highlighted_new_row_key, None);
    assert!(state.row_identity_anchors.is_empty());
    assert_eq!(state.preview_plan, None);
    assert_eq!(state.pending_status, PendingStatus::Idle);
    assert!(state.selected_rows.is_empty());
    assert_eq!(state.last_selected_index, None);
    assert_eq!(state.grid_selection, EMPTY_GRID_SELECTION);
    assert_eq!(state.grid_selection.mode, GridSelectionMode::None);
    assert_eq!(state.grid_selection.anchor, None);
    assert_eq!(state.grid_selection.focus, None);
    assert!(state.grid_selection.extra_ranges.is_empty());
    assert_eq!(state.editing_cell, None);
    assert!(state.enum_candidates.is_empty());
    assert_eq!(state.detail_row_index, None);
    assert!(!state.loading);
    assert_eq!(state.request_revision, 0);
    assert_eq!(state.loading_revision, None);
    assert_eq!(state.error, None);
    assert_eq!(state.visible_columns, None);
}

#[test]
fn empty_table_state_context_passthrough() {
    let ctx = context();
    let state = empty_table_state(Some(ctx.clone()));
    assert_eq!(state.context, Some(ctx));
    assert_eq!(empty_table_state(None).context, None);
}

#[test]
fn empty_table_state_fresh_collections_are_independent() {
    // Each call builds fresh maps, vectors and sets: mutating one panel's
    // state never leaks into another panel or into the shared constants.
    let mut first = empty_table_state(None);
    let second = empty_table_state(None);
    first.selected_rows.insert(3);
    first.columns.push(col("id"));
    first.keyset.mode = PagingMode::Keyset;
    first.grid_selection.mode = GridSelectionMode::Cell;
    assert!(second.selected_rows.is_empty());
    assert!(second.columns.is_empty());
    assert_eq!(second.keyset.mode, PagingMode::Offset);
    assert_eq!(second.grid_selection.mode, GridSelectionMode::None);
    assert_eq!(EMPTY_KEYSET_PAGING_STATE.mode, PagingMode::Offset);
    assert_eq!(EMPTY_GRID_SELECTION.mode, GridSelectionMode::None);
}

#[test]
fn empty_table_state_default_trait_matches() {
    assert_eq!(TableState::default(), empty_table_state(None));
}

#[test]
fn build_table_context_maps_all_fields() {
    let target = TableTarget {
        db_session_id: "sess-9".to_owned(),
        table: "users".to_owned(),
        connection_id: Some("conn-1".to_owned()),
        driver_type: Some("postgres".to_owned()),
        database: Some("app_db".to_owned()),
        schema: Some("public".to_owned()),
    };
    assert_eq!(build_table_context(&target), context());
}

#[test]
fn build_table_context_absent_optionals_are_null() {
    let target = TableTarget {
        db_session_id: "sess-9".to_owned(),
        table: "users".to_owned(),
        connection_id: None,
        driver_type: None,
        database: None,
        schema: None,
    };
    let built = build_table_context(&target);
    assert_eq!(built.connection_id, None);
    assert_eq!(built.db_session_id, "sess-9");
    assert_eq!(built.driver_type, None);
    assert_eq!(built.database, None);
    assert_eq!(built.schema, None);
    assert_eq!(built.table, "users");
}
