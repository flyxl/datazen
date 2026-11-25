//! T390 independent audit (tester): fresh vectors — an update with
//! changed columns after one without (the joined-col form), a composite
//! three-part pk, mixed statements under each filter face, the preview
//! text's `previewSql || sql` precedence with both empty, and an
//! operation filter matching nothing in a non-empty list.

use serde_json::json;

use datazen_ui::views::data_sync::client_sql_preview::{
    build_client_sql_preview, filter_statements_by_op, statements_to_preview_text,
    DataSyncSqlStatement, OpFilter,
};
use datazen_ui::views::{
    DataSyncMappingStatus, DataSyncOperation, DataSyncRowChange, DataSyncTableResult, SyncOptions,
};

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

#[test]
fn an_update_with_columns_after_one_without_keeps_both_forms() {
    let mut ellipsis = change(DataSyncOperation::Update, true, vec![json!(1)]);
    let mut with_cols = change(DataSyncOperation::Update, true, vec![json!(2)]);
    with_cols.changed_columns = vec!["name".to_owned()];
    let tables = vec![DataSyncTableResult {
        source_table: "s".to_owned(),
        target_table: "t".to_owned(),
        status: DataSyncMappingStatus::Matched,
        rows: vec![ellipsis, with_cols],
        ..Default::default()
    }];
    let text = build_client_sql_preview(&tables, &SyncOptions::default());
    assert_eq!(
        text,
        "-- UPDATE t SET … WHERE pk IN (1)\n-- UPDATE t SET name WHERE pk IN (2)"
    );
}

#[test]
fn a_three_part_pk_joins_all_cells() {
    let rows = vec![change(
        DataSyncOperation::Insert,
        true,
        vec![json!("a"), json!(12), json!(null)],
    )];
    let tables = vec![DataSyncTableResult {
        source_table: "s".to_owned(),
        target_table: "t".to_owned(),
        status: DataSyncMappingStatus::Matched,
        rows,
        ..Default::default()
    }];
    let text = build_client_sql_preview(&tables, &SyncOptions::default());
    assert_eq!(text, "-- INSERT INTO t (pk: a, 12, NULL)");
}

#[test]
fn each_filter_face_matches_only_its_own_statements() {
    let statements = vec![
        statement(DataSyncOperation::Insert, "-- i1", "INSERT 1"),
        statement(DataSyncOperation::Insert, "-- i2", "INSERT 2"),
        statement(DataSyncOperation::Update, "-- u1", "UPDATE 1"),
        statement(DataSyncOperation::Delete, "", "DELETE 1"),
    ];
    for (filter, expected) in [
        (OpFilter::Op(DataSyncOperation::Insert), "-- i1\n-- i2"),
        (OpFilter::Op(DataSyncOperation::Update), "-- u1"),
        (OpFilter::Op(DataSyncOperation::Delete), "DELETE 1"),
    ] {
        let text = statements_to_preview_text(&statements, filter);
        assert_eq!(text, expected);
    }
    // All renders them in order.
    let text = statements_to_preview_text(&statements, OpFilter::All);
    assert_eq!(text, "-- i1\n-- i2\n-- u1\nDELETE 1");
}

#[test]
fn both_preview_and_sql_empty_renders_an_empty_line() {
    // `previewSql || sql`: with both empty the statement contributes an
    // empty line to the join (never the placeholder — one statement
    // exists).
    let statements = vec![statement(DataSyncOperation::Insert, "", "")];
    let text = statements_to_preview_text(&statements, OpFilter::All);
    assert_eq!(text, "");
}

#[test]
fn an_operation_filter_matching_nothing_in_a_nonempty_list_placeholders() {
    let statements = vec![statement(DataSyncOperation::Insert, "-- i1", "INSERT 1")];
    let filtered = filter_statements_by_op(&statements, OpFilter::Op(DataSyncOperation::Update));
    assert!(filtered.is_empty());
    let text = statements_to_preview_text(&statements, OpFilter::Op(DataSyncOperation::Update));
    assert_eq!(text, "-- (no statements)");
}
