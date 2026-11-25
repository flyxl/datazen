//! T041 independent audit: React `stores/tableData/connectionState.ts`
//! parity, re-derived from the React source rather than from the port.
//!
//! Pins, independently of the coder's tests:
//! - `rows_to_records`: positional mapping, short-row null padding, surplus
//!   drop, explicit-null passthrough, duplicate-column last-wins (the
//!   `forEach` overwrite, matched by the `HashMap` insert);
//! - `to_cell_value`: `None` and `Some(Null)` both collapse to null, every
//!   other JSON shape passes through unchanged;
//! - `extract_error_message`: verbatim return (no trim) for every passing
//!   branch, blank rejection (`""`, whitespace-only) for every branch,
//!   fallback returned verbatim, `None` falls back;
//! - `empty_table_state`: every field's initial value, context passthrough,
//!   per-call collection independence, `Default` equivalence;
//! - `build_table_context`: all five optional fields, present / absent /
//!   mixed;
//! - the four dependency mirrors: `EMPTY_KEYSET_PAGING_STATE` and
//!   `EMPTY_GRID_SELECTION` field-by-field, every enum variant present
//!   (all 8 `KeysetUnavailableReason`, all 8 `CursorValue` kinds, all 3
//!   `PendingStatus` / `FilterMode` / `ChangeSeverity`, all 4
//!   `DefaultOrderOverride`, both `PagingMode`, all 3 `GridSelectionMode`),
//!   and constructibility of the change-plan types.

use std::error::Error;
use std::fmt;

use serde_json::{json, Value};

use datazen_ui::table_data_connection_state::{
    build_table_context, empty_table_state, extract_error_message, rows_to_records, to_cell_value,
    CellCoord, CellRange, ChangeSeverity, ChangeWarning, ColumnSchema, CountStrategy, CursorValue,
    DefaultOrderOverride, EditingCell, ErrorMessageSource, FilterCondition, FilterLogic,
    FilterMode, GridSelection, GridSelectionMode, KeysetCursor, KeysetPagingState,
    KeysetUnavailableReason, OrderColumn, PagingMode, PendingInsert, PendingRowChange,
    PendingStatus, PlannedStatement, RowChangePlan, SortCondition, TableChangeContext, TableState,
    TableTarget, WriteExpressionId, EMPTY_GRID_SELECTION, EMPTY_KEYSET_PAGING_STATE,
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

fn full_context() -> TableChangeContext {
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

// ── rows_to_records ────────────────────────────────────────────────────────

#[test]
fn audit_rows_to_records_short_row_pads_null() {
    // React: `record[col.name] = row[i] ?? null` — a row shorter than the
    // columns leaves the missing cells null.
    let columns = [col("id"), col("name"), col("age")];
    let rows = vec![vec![json!(7)], vec![]];
    let records = rows_to_records(&columns, &rows);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["id"], json!(7));
    assert_eq!(records[0]["name"], Value::Null);
    assert_eq!(records[0]["age"], Value::Null);
    assert_eq!(records[1]["id"], Value::Null);
    assert_eq!(records[1]["name"], Value::Null);
    assert_eq!(records[1]["age"], Value::Null);
}

#[test]
fn audit_rows_to_records_long_row_drops_surplus() {
    // A row longer than the columns: the record only ever has the columns'
    // keys — surplus values never reach it.
    let columns = [col("id"), col("name")];
    let rows = vec![vec![json!(1), json!("a"), json!("surplus"), json!(true)]];
    let records = rows_to_records(&columns, &rows);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].len(), 2);
    assert_eq!(records[0]["id"], json!(1));
    assert_eq!(records[0]["name"], json!("a"));
    assert!(!records[0].contains_key("surplus"));
}

#[test]
fn audit_rows_to_records_explicit_null_stays_null() {
    // An explicit null cell is null in the record — same output as padding,
    // but through the value path, not the missing-index path.
    let columns = [col("a"), col("b")];
    let rows = vec![vec![Value::Null, json!(2)]];
    let records = rows_to_records(&columns, &rows);
    assert_eq!(records[0]["a"], Value::Null);
    assert_eq!(records[0]["b"], json!(2));
}

#[test]
fn audit_rows_to_records_duplicate_column_last_wins() {
    // React `forEach` overwrites `record[col.name]`; the HashMap insert
    // must do the same, and the record keeps a single key.
    let columns = [col("x"), col("x")];
    let rows = vec![vec![json!("first"), json!("second")]];
    let records = rows_to_records(&columns, &rows);
    assert_eq!(records[0].len(), 1);
    assert_eq!(records[0]["x"], json!("second"));
}

#[test]
fn audit_rows_to_records_empty_inputs() {
    assert!(rows_to_records(&[col("id")], &[]).is_empty());
    let records = rows_to_records(&[], &[vec![json!(1)], vec![]]);
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.is_empty()));
}

#[test]
fn audit_rows_to_records_preserves_row_order_and_nesting() {
    let columns = [col("id"), col("meta")];
    let rows = vec![
        vec![json!(2), json!({"k": [1, 2]})],
        vec![json!(1), json!(null)],
    ];
    let records = rows_to_records(&columns, &rows);
    assert_eq!(records[0]["id"], json!(2));
    assert_eq!(records[0]["meta"], json!({"k": [1, 2]}));
    assert_eq!(records[1]["id"], json!(1));
    assert_eq!(records[1]["meta"], Value::Null);
}

// ── to_cell_value ──────────────────────────────────────────────────────────

#[test]
fn audit_to_cell_value_none_and_some_null_both_null() {
    // React: `null`/`undefined` → null. Rust: `None` is both, and an
    // explicit `Some(Value::Null)` must land in the same place.
    assert_eq!(to_cell_value(None), Value::Null);
    assert_eq!(to_cell_value(Some(Value::Null)), Value::Null);
}

#[test]
fn audit_to_cell_value_passthrough_all_json_shapes() {
    assert_eq!(to_cell_value(Some(json!("s"))), json!("s"));
    assert_eq!(to_cell_value(Some(json!(0))), json!(0));
    assert_eq!(to_cell_value(Some(json!(-3.5))), json!(-3.5));
    assert_eq!(to_cell_value(Some(json!(false))), json!(false));
    assert_eq!(to_cell_value(Some(json!([]))), json!([]));
    assert_eq!(to_cell_value(Some(json!([null, 1]))), json!([null, 1]));
    assert_eq!(to_cell_value(Some(json!({}))), json!({}));
    assert_eq!(
        to_cell_value(Some(json!({"n": {"m": [true]}}))),
        json!({"n": {"m": [true]}})
    );
}

// ── extract_error_message ──────────────────────────────────────────────────

#[test]
fn audit_extract_text_returned_verbatim_untrimmed() {
    // A passing message is returned as-is; only the blank check trims.
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::Text("  boom  ")), "fb"),
        "  boom  "
    );
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::Text("\tx\n")), "fb"),
        "\tx\n"
    );
}

#[test]
fn audit_extract_text_blank_variants_fall_back() {
    for blank in ["", " ", "   \t\n\r "] {
        assert_eq!(
            extract_error_message(Some(ErrorMessageSource::Text(blank)), "fb"),
            "fb",
            "blank text {blank:?} must fall back"
        );
    }
}

#[test]
fn audit_extract_error_display_verbatim_and_blank() {
    let error = MsgError("  refused  ");
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::Error(&error)), "fb"),
        "  refused  "
    );
    for blank in [MsgError(""), MsgError("  "), MsgError("\n")] {
        assert_eq!(
            extract_error_message(Some(ErrorMessageSource::Error(&blank)), "fb"),
            "fb",
            "blank error message must fall back"
        );
    }
}

#[test]
fn audit_extract_message_field_verbatim_and_blank() {
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::MessageField("  said so  ")), "fb"),
        "  said so  "
    );
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::MessageField("")), "fb"),
        "fb"
    );
    assert_eq!(
        extract_error_message(Some(ErrorMessageSource::MessageField(" \t ")), "fb"),
        "fb"
    );
}

#[test]
fn audit_extract_none_and_fallback_verbatim() {
    // `None` is everything else (non-string, non-Error, no message field).
    assert_eq!(extract_error_message(None, "fb"), "fb");
    // The fallback itself is never trimmed either.
    assert_eq!(extract_error_message(None, "  fb  "), "  fb  ");
    assert_eq!(extract_error_message(None, ""), "");
}

// ── empty_table_state ──────────────────────────────────────────────────────

#[test]
fn audit_empty_state_every_field() {
    // Re-derived from React `emptyTableState`, field by field.
    let state = empty_table_state(None);
    assert_eq!(state.context, None);
    assert!(state.columns.is_empty());
    assert!(state.rows.is_empty());
    assert_eq!(state.total_rows, 0);
    assert_eq!(state.total_rows_kind, CountStrategy::Exact);
    assert_eq!(state.default_order_override, DefaultOrderOverride::Inherit);
    assert_eq!(state.page, 0);
    assert_eq!(state.page_size, 50);
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
fn audit_empty_state_context_passthrough_and_isolation() {
    let ctx = full_context();
    let with_ctx = empty_table_state(Some(ctx.clone()));
    assert_eq!(with_ctx.context, Some(ctx));
    // A second call with no context is unaffected by the first.
    assert_eq!(empty_table_state(None).context, None);
}

#[test]
fn audit_empty_state_collections_are_per_call_fresh() {
    let mut first = empty_table_state(None);
    first.selected_rows.insert(7);
    first.columns.push(col("id"));
    first.pending_changes.insert(
        "k".to_owned(),
        PendingRowChange {
            row_index: None,
            row_identity: Default::default(),
            original_values: Default::default(),
            current_values: Default::default(),
            changed_columns: vec![],
            delete_marked: false,
            insert_marked: false,
            expression_values: None,
        },
    );
    first
        .enum_candidates
        .insert("status".to_owned(), vec!["a".to_owned()]);
    first.keyset.mode = PagingMode::Keyset;
    first.keyset.stack.push(KeysetCursor {
        boundary: None,
        order_signature: "id:asc".to_owned(),
        filter_signature: "and()".to_owned(),
    });
    first.grid_selection.mode = GridSelectionMode::Cell;
    first.grid_selection.extra_ranges.push(CellRange {
        start: CellCoord {
            row_index: 0,
            column_name: "id".to_owned(),
        },
        end: CellCoord {
            row_index: 0,
            column_name: "id".to_owned(),
        },
    });

    let second = empty_table_state(None);
    assert!(second.selected_rows.is_empty());
    assert!(second.columns.is_empty());
    assert!(second.pending_changes.is_empty());
    assert!(second.enum_candidates.is_empty());
    assert_eq!(second.keyset.mode, PagingMode::Offset);
    assert!(second.keyset.stack.is_empty());
    assert_eq!(second.grid_selection.mode, GridSelectionMode::None);
    assert!(second.grid_selection.extra_ranges.is_empty());
    // The shared constants are untouched by the mutation above.
    assert_eq!(EMPTY_KEYSET_PAGING_STATE.mode, PagingMode::Offset);
    assert!(EMPTY_KEYSET_PAGING_STATE.stack.is_empty());
    assert_eq!(EMPTY_GRID_SELECTION.mode, GridSelectionMode::None);
    assert!(EMPTY_GRID_SELECTION.extra_ranges.is_empty());
}

#[test]
fn audit_empty_state_default_trait_matches() {
    assert_eq!(TableState::default(), empty_table_state(None));
}

#[test]
fn audit_empty_state_fields_are_mutable_to_react_values() {
    // The mirror struct must accept every React field value a store would
    // later write (estimated kind, raw-sql mode, previewing, editing cell).
    let mut state = empty_table_state(None);
    state.total_rows_kind = CountStrategy::Estimated;
    state.page = 3;
    state.filter_mode = FilterMode::RawSql;
    state.pending_status = PendingStatus::Previewing;
    state.editing_cell = Some(EditingCell {
        row: 2,
        col: "name".to_owned(),
    });
    state.filters.push(FilterCondition {
        column: "age".to_owned(),
        operator: "between".to_owned(),
        value: Some(json!([18, 65])),
        connector: Some(FilterLogic::Or),
    });
    state.sorts.push(SortCondition {
        column: "name".to_owned(),
        descending: true,
    });
    assert_eq!(state.total_rows_kind, CountStrategy::Estimated);
    assert_eq!(state.page, 3);
    assert_eq!(state.filter_mode, FilterMode::RawSql);
    assert_eq!(state.pending_status, PendingStatus::Previewing);
    assert_eq!(
        state.editing_cell,
        Some(EditingCell {
            row: 2,
            col: "name".to_owned()
        })
    );
    assert_eq!(state.filters.len(), 1);
    assert_eq!(state.sorts.len(), 1);
}

// ── build_table_context ────────────────────────────────────────────────────

#[test]
fn audit_build_context_all_fields_present() {
    let target = TableTarget {
        db_session_id: "sess-9".to_owned(),
        table: "users".to_owned(),
        connection_id: Some("conn-1".to_owned()),
        driver_type: Some("postgres".to_owned()),
        database: Some("app_db".to_owned()),
        schema: Some("public".to_owned()),
    };
    assert_eq!(build_table_context(&target), full_context());
}

#[test]
fn audit_build_context_all_optionals_absent_are_null() {
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

#[test]
fn audit_build_context_mixed_optionals() {
    // `?? null` is per-field: one present field does not fill the others.
    let target = TableTarget {
        db_session_id: "s".to_owned(),
        table: "t".to_owned(),
        connection_id: None,
        driver_type: Some("mysql".to_owned()),
        database: None,
        schema: Some("dbo".to_owned()),
    };
    let built = build_table_context(&target);
    assert_eq!(built.connection_id, None);
    assert_eq!(built.driver_type, Some("mysql".to_owned()));
    assert_eq!(built.database, None);
    assert_eq!(built.schema, Some("dbo".to_owned()));
}

// ── dependency mirrors ─────────────────────────────────────────────────────

#[test]
fn audit_mirror_empty_keyset_paging_state_fields() {
    // React `emptyKeysetPagingState`, field by field.
    let empty = EMPTY_KEYSET_PAGING_STATE;
    assert_eq!(empty.mode, PagingMode::Offset);
    assert!(empty.order_columns.is_empty());
    assert!(empty.stack.is_empty());
    assert_eq!(empty.anchor_page, 0);
    assert_eq!(empty.unavailable, None);
    assert!(!empty.drift_suspected);
    assert_eq!(empty.next_after_key, None);
    // A populated keyset state is constructible and distinct.
    let populated = KeysetPagingState {
        mode: PagingMode::Keyset,
        order_columns: vec![OrderColumn {
            column: "id".to_owned(),
            descending: false,
        }],
        stack: vec![KeysetCursor {
            boundary: Some(vec![CursorValue::Integer(7)]),
            order_signature: "id:asc".to_owned(),
            filter_signature: "and()".to_owned(),
        }],
        anchor_page: 2,
        unavailable: Some(KeysetUnavailableReason::StaleCursor),
        drift_suspected: true,
        next_after_key: Some(vec![CursorValue::String("k".to_owned())]),
    };
    assert_ne!(populated, EMPTY_KEYSET_PAGING_STATE);
    assert_eq!(
        populated.stack[0].boundary,
        Some(vec![CursorValue::Integer(7)])
    );
}

#[test]
fn audit_mirror_empty_grid_selection_fields() {
    // React `EMPTY_GRID_SELECTION`, field by field.
    assert_eq!(EMPTY_GRID_SELECTION.anchor, None);
    assert_eq!(EMPTY_GRID_SELECTION.focus, None);
    assert!(EMPTY_GRID_SELECTION.extra_ranges.is_empty());
    assert_eq!(EMPTY_GRID_SELECTION.mode, GridSelectionMode::None);
    let selected = GridSelection {
        anchor: Some(CellCoord {
            row_index: 1,
            column_name: "id".to_owned(),
        }),
        focus: Some(CellCoord {
            row_index: 3,
            column_name: "name".to_owned(),
        }),
        extra_ranges: vec![],
        mode: GridSelectionMode::Cell,
    };
    assert_ne!(selected, EMPTY_GRID_SELECTION);
}

#[test]
fn audit_mirror_all_enum_variants_present() {
    // Every variant the React unions declare must exist on the Rust side.
    let counts = [
        CountStrategy::Exact,
        CountStrategy::Estimated,
        CountStrategy::Unsupported,
    ];
    assert_eq!(counts.len(), 3);
    assert_ne!(counts[0], counts[1]);
    assert_ne!(counts[1], counts[2]);

    assert_ne!(PagingMode::Offset, PagingMode::Keyset);

    let reasons = [
        KeysetUnavailableReason::DriverUnsupported,
        KeysetUnavailableReason::NoStableOrder,
        KeysetUnavailableReason::NullableKeyColumn,
        KeysetUnavailableReason::NullBoundary,
        KeysetUnavailableReason::PredicateUnavailable,
        KeysetUnavailableReason::NoPaginationClause,
        KeysetUnavailableReason::StaleCursor,
        KeysetUnavailableReason::NotAdjacent,
    ];
    assert_eq!(reasons.len(), 8);
    for (i, a) in reasons.iter().enumerate() {
        for (j, b) in reasons.iter().enumerate() {
            assert_eq!(a == b, i == j, "reasons[{i}] vs reasons[{j}]");
        }
    }

    assert_ne!(PendingStatus::Idle, PendingStatus::Previewing);
    assert_ne!(PendingStatus::Previewing, PendingStatus::Committing);
    assert_ne!(PendingStatus::Idle, PendingStatus::Committing);

    assert_ne!(FilterMode::Structured, FilterMode::AnyColumn);
    assert_ne!(FilterMode::AnyColumn, FilterMode::RawSql);

    let overrides = [
        DefaultOrderOverride::Inherit,
        DefaultOrderOverride::PrimaryKeyAsc,
        DefaultOrderOverride::PrimaryKeyDesc,
        DefaultOrderOverride::None,
    ];
    assert_eq!(overrides.len(), 4);

    assert_ne!(GridSelectionMode::None, GridSelectionMode::Cell);
    assert_ne!(GridSelectionMode::Cell, GridSelectionMode::Row);

    assert_ne!(FilterLogic::And, FilterLogic::Or);

    assert_ne!(ChangeSeverity::Info, ChangeSeverity::Warning);
    assert_ne!(ChangeSeverity::Warning, ChangeSeverity::Error);

    // `WriteExpressionId` is a closed single-variant set in React.
    let _expr = WriteExpressionId::CurrentTimestamp;
}

#[test]
fn audit_mirror_all_cursor_value_kinds_present() {
    // React `CursorValue`: all 8 tagged kinds.
    let values = [
        CursorValue::Null,
        CursorValue::Bool(true),
        CursorValue::Integer(-9),
        CursorValue::Float(1.5),
        CursorValue::String("s".to_owned()),
        CursorValue::Bytes(vec![1, 2, 3]),
        CursorValue::Timestamp("2026-01-01T00:00:00Z".to_owned()),
        CursorValue::Json(json!({"a": 1})),
    ];
    assert_eq!(values.len(), 8);
    assert_eq!(values[0], CursorValue::Null);
    assert_eq!(values[2], CursorValue::Integer(-9));
    assert_eq!(values[5], CursorValue::Bytes(vec![1, 2, 3]));
    assert_ne!(values[2], CursorValue::Float(-9.0));
}

#[test]
fn audit_mirror_change_plan_types_construct() {
    // The `RowChangePlan` closure: statement, warning, plan — with
    // `inserts` always present (React: required, never a missing key).
    let statement = PlannedStatement {
        row_identity: [("id".to_owned(), json!(1))].into_iter().collect(),
        original_values: [("name".to_owned(), json!("a"))].into_iter().collect(),
        current_values: [("name".to_owned(), json!("b"))].into_iter().collect(),
        changed_columns: vec!["name".to_owned()],
        expression_values: Some(
            [("updated_at".to_owned(), WriteExpressionId::CurrentTimestamp)]
                .into_iter()
                .collect(),
        ),
        sql_template: "UPDATE t SET name = ? WHERE id = ?".to_owned(),
        parameter_summary: vec!["'b'".to_owned()],
    };
    let plan = RowChangePlan {
        plan_id: "p1".to_owned(),
        fingerprint: "fp".to_owned(),
        config_revision: Some(4),
        table: full_context(),
        updates: vec![statement.clone()],
        deletes: vec![],
        inserts: vec![],
        warnings: vec![ChangeWarning {
            code: "W1".to_owned(),
            message: "watch out".to_owned(),
            severity: ChangeSeverity::Warning,
        }],
    };
    assert_eq!(plan.updates.len(), 1);
    assert!(plan.inserts.is_empty());
    assert_eq!(plan.warnings[0].severity, ChangeSeverity::Warning);
    assert_eq!(plan.config_revision, Some(4));
    assert_eq!(plan.updates[0], statement);

    let insert = PendingInsert {
        draft_id: "d1".to_owned(),
        values: [("name".to_owned(), Value::Null)].into_iter().collect(),
    };
    // Present key with null = written as SQL NULL (membership is intent).
    assert_eq!(insert.values["name"], Value::Null);
    assert!(!insert.values.contains_key("id"));
}
