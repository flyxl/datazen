//! T390: `windows/data-sync/clientSqlPreview.ts` port — the statement
//! filter, the preview text join (`previewSql || sql`), and the
//! no-wire row projection with its option / selection / status gates.

use serde_json::json;

use datazen_ui::views::data_sync::client_sql_preview::{
    build_client_sql_preview, filter_statements_by_op, statements_to_preview_text,
    DataSyncSqlStatement, OpFilter,
};
use datazen_ui::views::{
    DataSyncMappingStatus, DataSyncOperation, DataSyncRowChange, DataSyncTableResult, SyncOptions,
};

fn options(insert: bool, update: bool, delete: bool) -> SyncOptions {
    SyncOptions {
        insert,
        update,
        delete,
        ..Default::default()
    }
}

fn change(operation: DataSyncOperation, selected: bool, key: Vec<serde_json::Value>) -> DataSyncRowChange {
    DataSyncRowChange {
        operation,
        key,
        source_row: None,
        target_row: None,
        changed_columns: Vec::new(),
        selected,
    }
}

fn table(source: &str, target: &str, status: DataSyncMappingStatus, rows: Vec<DataSyncRowChange>) -> DataSyncTableResult {
    DataSyncTableResult {
        source_table: source.to_owned(),
        target_table: target.to_owned(),
        status,
        rows,
        ..Default::default()
    }
}

fn statement(operation: DataSyncOperation, preview_sql: &str, sql: &str) -> DataSyncSqlStatement {
    DataSyncSqlStatement {
        table: "t".to_owned(),
        operation,
        sql: sql.to_owned(),
        preview_sql: preview_sql.to_owned(),
        parameters: Vec::new(),
        row_key: Vec::new(),
    }
}

// ---- the statement filter and text ----

#[test]
fn the_all_filter_keeps_every_statement() {
    let statements = vec![
        statement(DataSyncOperation::Insert, "-- i1", "INSERT 1"),
        statement(DataSyncOperation::Delete, "", "DELETE 1"),
    ];
    let filtered = filter_statements_by_op(&statements, OpFilter::All);
    assert_eq!(filtered.len(), 2);
}

#[test]
fn an_operation_filter_keeps_only_its_own() {
    let statements = vec![
        statement(DataSyncOperation::Insert, "-- i1", "INSERT 1"),
        statement(DataSyncOperation::Update, "-- u1", "UPDATE 1"),
        statement(DataSyncOperation::Delete, "", "DELETE 1"),
    ];
    let filtered = filter_statements_by_op(&statements, OpFilter::Op(DataSyncOperation::Insert));
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].operation, DataSyncOperation::Insert);
}

#[test]
fn the_preview_text_prefers_preview_sql_and_joins_on_newlines() {
    let statements = vec![
        statement(DataSyncOperation::Insert, "-- i1", "INSERT 1"),
        statement(DataSyncOperation::Delete, "", "DELETE 1"),
    ];
    let text = statements_to_preview_text(&statements, OpFilter::All);
    assert_eq!(text, "-- i1\nDELETE 1");
}

#[test]
fn an_empty_filter_result_renders_the_placeholder() {
    let statements = vec![statement(DataSyncOperation::Insert, "-- i1", "INSERT 1")];
    let text = statements_to_preview_text(&statements, OpFilter::Op(DataSyncOperation::Delete));
    assert_eq!(text, "-- (no statements)");
    assert_eq!(statements_to_preview_text(&[], OpFilter::All), "-- (no statements)");
}

// ---- the no-wire projection ----

#[test]
fn every_operation_renders_its_own_comment_line() {
    let rows = vec![
        change(DataSyncOperation::Insert, true, vec![json!(1)]),
        change(
            DataSyncOperation::Update,
            true,
            vec![json!("a"), json!(true)],
        ),
        change(DataSyncOperation::Delete, true, vec![json!(null)]),
    ];
    let tables = vec![table("src", "dst", DataSyncMappingStatus::Matched, rows)];
    let text = build_client_sql_preview(&tables, &options(true, true, true));
    assert_eq!(
        text,
        "-- INSERT INTO dst (pk: 1)\n-- UPDATE dst SET … WHERE pk IN (a, true)\n-- DELETE FROM dst WHERE pk IN (NULL)"
    );
}

#[test]
fn the_target_falls_back_to_the_source_table() {
    let rows = vec![change(DataSyncOperation::Insert, true, vec![json!(1)])];
    let tables = vec![table("src", "", DataSyncMappingStatus::Matched, rows)];
    let text = build_client_sql_preview(&tables, &options(true, true, true));
    assert_eq!(text, "-- INSERT INTO src (pk: 1)");
}

#[test]
fn changed_columns_join_and_empty_uses_the_ellipsis() {
    let mut row = change(DataSyncOperation::Update, true, vec![json!(1)]);
    row.changed_columns = vec!["a".to_owned(), "b".to_owned()];
    let tables = vec![table("s", "t", DataSyncMappingStatus::Matched, vec![row])];
    let text = build_client_sql_preview(&tables, &options(true, true, true));
    assert!(text.contains("-- UPDATE t SET a, b WHERE pk IN (1)"), "cols joined: {text}");
}

#[test]
fn the_gates_skip_unselected_unchanged_and_disallowed_rows() {
    let rows = vec![
        change(DataSyncOperation::Insert, false, vec![json!(1)]), // unselected
        change(DataSyncOperation::Unchanged, true, vec![json!(2)]), // unchanged
        change(DataSyncOperation::Insert, true, vec![json!(3)]), // insert off
        change(DataSyncOperation::Update, true, vec![json!(4)]), // update off
    ];
    let tables = vec![table("s", "t", DataSyncMappingStatus::Matched, rows)];
    let text = build_client_sql_preview(&tables, &options(false, false, true));
    assert_eq!(text, "-- (no selected changes)");
    // With every gate open the insert renders.
    let text = build_client_sql_preview(&tables, &options(true, true, true));
    assert!(text.contains("INSERT INTO t (pk: 3)"));
    assert!(text.contains("UPDATE t SET … WHERE pk IN (4)"));
}

#[test]
fn non_matched_tables_are_skipped_entirely() {
    let rows = vec![change(DataSyncOperation::Insert, true, vec![json!(1)])];
    let tables = vec![
        table("a", "a", DataSyncMappingStatus::Incompatible, rows.clone()),
        table("b", "b", DataSyncMappingStatus::Disabled, rows.clone()),
        table("c", "c", DataSyncMappingStatus::UnmappedSource, rows.clone()),
    ];
    let text = build_client_sql_preview(&tables, &options(true, true, true));
    assert_eq!(text, "-- (no selected changes)");
}

#[test]
fn a_fully_empty_input_renders_the_placeholder() {
    let text = build_client_sql_preview(&[], &options(true, true, true));
    assert_eq!(text, "-- (no selected changes)");
}

#[test]
fn pk_cells_render_through_format_cell() {
    let rows = vec![change(
        DataSyncOperation::Insert,
        true,
        vec![json!({"nested": 1}), json!([1, 2]), json!(false), json!("x"), json!(null)],
    )];
    let tables = vec![table("s", "t", DataSyncMappingStatus::Matched, rows)];
    let text = build_client_sql_preview(&tables, &options(true, true, true));
    // formatCell: objects and arrays via JSON, booleans as words, strings
    // bare, null as NULL.
    assert_eq!(
        text,
        "-- INSERT INTO t (pk: {\"nested\":1}, [1,2], false, x, NULL)"
    );
}
