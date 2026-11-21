use serde_json::{json, Value};

use super::*;

fn row(operation: DataSyncOperation, key: Vec<Value>, selected: bool) -> DataSyncRowChange {
    DataSyncRowChange {
        operation,
        key,
        source_row: None,
        target_row: None,
        changed_columns: Vec::new(),
        selected,
    }
}

fn table(rows: Vec<DataSyncRowChange>) -> DataSyncTableResult {
    DataSyncTableResult {
        source_table: "users".to_owned(),
        target_table: "users_target".to_owned(),
        columns: Some(vec!["id".to_owned(), "name".to_owned()]),
        rows: Some(rows),
        ..Default::default()
    }
}

fn options() -> SyncOptions {
    SyncOptions { insert: true, update: true, delete: true }
}

#[test]
fn constants_and_operation_literals() {
    assert_eq!(PAGE_SIZE, 500);
    assert_eq!(DataSyncOperation::Insert.as_str(), "INSERT");
    assert_eq!(DataSyncOperation::Unchanged.as_str(), "UNCHANGED");
    assert_eq!(
        operation_badge_class(DataSyncOperation::Insert),
        "bg-green-500/15 text-green-700 dark:text-green-400"
    );
    assert_eq!(
        operation_badge_class(DataSyncOperation::Unchanged),
        "bg-surface-alt text-fg-muted"
    );
}

#[test]
fn cell_formatting_and_row_key_serialization() {
    assert_eq!(format_cell(None), "NULL");
    assert_eq!(format_cell(Some(&Value::Null)), "NULL");
    assert_eq!(format_cell(Some(&json!(true))), "true");
    assert_eq!(format_cell(Some(&json!("a"))), "a");
    assert_eq!(format_cell(Some(&json!([1, 2]))), "[1,2]");
    assert_eq!(format_cell(Some(&json!({"a": 1}))), "{\"a\":1}");
    assert_eq!(row_key_string(&[json!(1), json!("a")]), "[1,\"a\"]");
    assert_eq!(row_key_string(&[]), "[]");
}

#[test]
fn operation_allowed_matches_the_three_enabled_flags() {
    assert!(operation_allowed(DataSyncOperation::Insert, &options()));
    assert!(operation_allowed(DataSyncOperation::Update, &options()));
    assert!(operation_allowed(DataSyncOperation::Delete, &options()));
    assert!(!operation_allowed(DataSyncOperation::Unchanged, &options()));
    let none = SyncOptions::default();
    assert!(!operation_allowed(DataSyncOperation::Insert, &none));
}

#[test]
fn server_paging_detection_and_diff_rows() {
    let mut t = table(vec![
        row(DataSyncOperation::Insert, vec![json!(1)], false),
        row(DataSyncOperation::Unchanged, vec![json!(2)], false),
    ]);
    assert!(!server_paged(&t));
    t.page_size = Some(10);
    assert!(server_paged(&t));
    let mut t2 = table(Vec::new());
    t2.row_count = Some(0);
    assert!(server_paged(&t2));
    let rows = diff_rows(&t);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].operation, DataSyncOperation::Insert);
    assert!(diff_rows(&DataSyncTableResult::default()).is_empty());
}

#[test]
fn local_page_count_safe_page_and_slice() {
    let rows: Vec<DataSyncRowChange> = (0..5)
        .map(|index| row(DataSyncOperation::Insert, vec![json!(index)], false))
        .collect();
    let t = table(rows.clone());
    assert_eq!(page_count(&t, &rows, 2), 3, "ceil(5/2)");
    assert_eq!(page_count(&t, &rows, 500), 1);
    assert_eq!(page_count(&DataSyncTableResult::default(), &[], 2), 1, "never zero");
    assert_eq!(safe_page(&t, 9, 4, 3), 2, "local page clamps to the last page");

    let first = page_rows(&t, &rows, 0, 2);
    let last = page_rows(&t, &rows, 2, 2);
    assert_eq!(first.len(), 2);
    assert_eq!(last.len(), 1);
    assert_eq!(row_key_string(&first[0].key), "[0]");
    assert_eq!(row_key_string(&last[0].key), "[4]");
    assert!(page_rows(&t, &rows, 99, 2).is_empty());
}

#[test]
fn server_page_count_uses_row_count_and_keeps_all_rows() {
    let rows = vec![row(DataSyncOperation::Insert, vec![json!(1)], false)];
    let mut t = table(rows.clone());
    t.page_size = Some(25);
    t.row_count = Some(101);
    assert_eq!(page_count(&t, &rows, 2), 5, "ceil(101/25)");
    assert_eq!(safe_page(&t, 9, 3, 5), 3, "server page is used verbatim");
    assert_eq!(page_rows(&t, &rows, 3, 2).len(), 1, "server paging does not slice locally");
    t.row_count = None;
    assert_eq!(page_count(&t, &rows, 2), 1, "falls back to the loaded diff rows");
}

#[test]
fn max_cols_empty_state_and_footer_predicates() {
    let mut first = row(DataSyncOperation::Insert, vec![json!(1)], false);
    first.source_row = Some(vec![Some(json!("a"))]);
    let mut second = row(DataSyncOperation::Update, vec![json!(2)], false);
    second.target_row = Some(vec![Some(json!("a")), Some(json!("b"))]);
    assert_eq!(max_cols(&[first, second]), 2);
    assert_eq!(max_cols(&[]), 0);

    let t = table(Vec::new());
    assert!(is_empty_state(&[], &t, false));
    let mut server = t.clone();
    server.page_size = Some(10);
    assert!(!is_empty_state(&[], &server, true), "a server page may still have a next page");
    assert!(is_empty_state(&[], &server, false));
    assert!(footer_visible(1, &t) == false);
    assert!(footer_visible(2, &t));
    assert!(footer_visible(1, &server), "server paging always shows the footer");
    assert!(prev_disabled(&t, false, 0));
    assert!(!prev_disabled(&t, false, 1));
    assert!(next_disabled(&t, false, false, 2, 3));
    assert!(next_disabled(&server, false, false, 0, 5));
    assert!(next_disabled(&server, true, true, 0, 5), "page loading disables the next button");
}

#[test]
fn toggle_row_targets_the_full_row_by_key() {
    let rows = vec![
        row(DataSyncOperation::Insert, vec![json!(1)], false),
        row(DataSyncOperation::Update, vec![json!(2)], false),
    ];
    let t = table(rows.clone());
    let page = page_rows(&t, &rows, 0, 500);
    let next = toggle_row(&rows, &page, 1, true).expect("a match");
    assert!(next[1].selected);
    assert!(!next[0].selected);
    assert!(toggle_row(&rows, &page, 9, true).is_none(), "an out-of-page index is ignored");
    assert!(toggle_row(&rows, &[], 0, true).is_none());
}

#[test]
fn select_all_and_clear_all_respect_operation_allowed() {
    let rows = vec![
        row(DataSyncOperation::Insert, vec![json!(1)], false),
        row(DataSyncOperation::Update, vec![json!(2)], true),
        row(DataSyncOperation::Delete, vec![json!(3)], true),
        row(DataSyncOperation::Unchanged, vec![json!(4)], false),
    ];
    let mut opt = options();
    opt.delete = false;
    let selected = select_all_op(&rows, DataSyncOperation::Insert, &opt);
    assert!(selected[0].selected);
    let cleared = select_all_op(&rows, DataSyncOperation::Delete, &opt);
    assert!(!cleared[2].selected, "a disallowed operation cannot be selected");
    assert!(cleared[1].selected, "other operations are untouched");
    let cleared = clear_all_op(&rows, DataSyncOperation::Update);
    assert!(!cleared[1].selected);
    assert!(cleared[0].selected == false);
}

#[test]
fn row_selectable_follows_operation_and_options() {
    let insert = row(DataSyncOperation::Insert, vec![json!(1)], true);
    let delete = row(DataSyncOperation::Delete, vec![json!(2)], true);
    let unchanged = row(DataSyncOperation::Unchanged, vec![json!(3)], true);
    let mut opt = options();
    assert!(row_selectable(&insert, &opt));
    assert!(row_selectable(&delete, &opt));
    assert!(!row_selectable(&unchanged, &opt));
    opt.delete = false;
    assert!(!row_selectable(&delete, &opt), "DELETE needs the delete option");
}

#[test]
fn key_label_changed_flag_and_cell_views() {
    let mut changed = row(DataSyncOperation::Update, vec![json!(1), json!("a")], false);
    changed.source_row = Some(vec![Some(json!("old")), Some(json!("same"))]);
    changed.target_row = Some(vec![Some(json!("new")), Some(json!("same"))]);
    changed.changed_columns = vec!["id".to_owned()];
    let t = table(vec![changed.clone()]);
    assert_eq!(key_label(&changed), "1 · a");
    assert!(is_changed(&changed, &t, 0));
    assert!(!is_changed(&changed, &t, 1));
    assert_eq!(
        cell_view(&changed, &t, 0),
        CellView::Update { source: "old".to_owned(), target: "new".to_owned(), changed: true }
    );
    assert_eq!(
        cell_view(&changed, &t, 1),
        CellView::Update { source: "same".to_owned(), target: "same".to_owned(), changed: false }
    );

    let mut insert = row(DataSyncOperation::Insert, vec![json!(1)], false);
    insert.source_row = Some(vec![Some(json!("new"))]);
    assert_eq!(cell_view(&insert, &t, 0), CellView::Insert { text: "new".to_owned() });
    let mut delete = row(DataSyncOperation::Delete, vec![json!(1)], false);
    delete.target_row = Some(vec![Some(json!("old"))]);
    assert_eq!(cell_view(&delete, &t, 0), CellView::Delete { text: "old".to_owned() });
    let mut missing = row(DataSyncOperation::Insert, vec![json!(1)], false);
    missing.source_row = Some(vec![None]);
    assert_eq!(cell_view(&missing, &t, 0), CellView::Insert { text: "NULL".to_owned() });
    assert_eq!(cell_view(&insert, &t, 9), CellView::Insert { text: "NULL".to_owned() });
}

#[test]
fn dom_gap_is_pinned() {
    assert_eq!(DIFF_DETAIL_DOM_GAP.len(), 4);
    assert!(DIFF_DETAIL_DOM_GAP.contains(&"Checkbox onChange"));
    assert!(DIFF_DETAIL_DOM_GAP.contains(&"pagination Button onClick"));
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::diff_rows;
    let _ = crate::server_paged;
    let _ = crate::page_count;
    let _ = crate::safe_page;
    let _ = crate::page_rows;
    let _ = crate::max_cols;
    let _ = crate::is_empty_state;
    let _ = crate::footer_visible;
    let _ = crate::prev_disabled;
    let _ = crate::next_disabled;
    let _ = crate::toggle_row;
    let _ = crate::select_all_op;
    let _ = crate::clear_all_op;
    let _ = crate::row_selectable;
    let _ = crate::key_label;
    let _ = crate::is_changed;
    let _ = crate::cell_view;
    let _ = crate::format_cell;
    let _ = crate::operation_allowed;
    let _ = crate::operation_badge_class;
    let _ = crate::row_key_string;
    assert_eq!(crate::PAGE_SIZE, 500);
    let _: Option<crate::DataSyncOperation> = None;
    let _: Option<crate::DataSyncRowChange> = None;
    let _: Option<crate::DataSyncTableResult> = None;
    let _: Option<crate::SyncOptions> = None;
    let _: Option<crate::CellView> = None;
}
