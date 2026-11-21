use std::collections::HashSet;

use crate::types::ColumnSchema;
use crate::{
    ClipboardDelimiter, ClipboardMatrix, GridClipboardAnchor, GridClipboardRange,
    InboundTableChangeContext as TableChangeContext, PasteProblemCode,
};

use super::*;

fn mx(rows: &[&[&str]]) -> ClipboardMatrix {
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| row.iter().map(|value| (*value).to_owned()).collect())
        .collect();
    let quoted: Vec<Vec<bool>> = rows.iter().map(|row| vec![false; row.len()]).collect();
    ClipboardMatrix {
        byte_length: cells.iter().flatten().map(|value| value.len()).sum(),
        cells,
        quoted,
        delimiter: ClipboardDelimiter::Tab,
        multiline: rows.len() > 1,
    }
}

fn quoted_mx(rows: &[&[&str]], flags: &[&[bool]]) -> ClipboardMatrix {
    let mut matrix = mx(rows);
    matrix.quoted = flags.iter().map(|row| row.to_vec()).collect();
    matrix
}

fn schema(name: &str, nullable: bool, auto_increment: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: "text".to_owned(),
        nullable,
        default_value: None,
        is_primary_key: None,
        is_auto_increment: Some(auto_increment),
        comment: None,
    }
}

fn options(matrix: ClipboardMatrix) -> PastePlanOptions {
    PastePlanOptions {
        matrix,
        anchor: GridClipboardAnchor {
            row_index: 0,
            column_name: "id".to_owned(),
        },
        column_order: vec!["id".to_owned(), "name".to_owned()],
        columns: vec![schema("id", false, false), schema("name", true, false)],
        region: None,
        page_row_count: 5,
        overflow: PasteOverflow::Expand,
        null_literal: "NULL".to_owned(),
        pending_cell_keys: HashSet::new(),
        source: None,
        context: None,
    }
}

fn context() -> TableChangeContext {
    TableChangeContext {
        connection_id: "conn".to_owned(),
        db_session_id: "session".to_owned(),
        driver_type: "postgres".to_owned(),
        database: "shop".to_owned(),
        schema: Some("public".to_owned()),
        table: "customers".to_owned(),
    }
}

#[test]
fn literals_and_keys_are_pinned() {
    assert_eq!(PasteOverflow::Expand.as_str(), "expand");
    assert_eq!(PasteOverflow::Clip.as_str(), "clip");
    assert_eq!(PasteAlignment::ByName.as_str(), "byName");
    assert_eq!(PasteAlignment::ByPosition.as_str(), "byPosition");
    assert_eq!(PasteAlignment::Unknown.as_str(), "unknown");
    assert_eq!(
        ColumnWritabilityReason::AutoIncrement.as_str(),
        "auto-increment"
    );
    assert_eq!(
        ColumnWritabilityReason::UnknownColumn.as_str(),
        "unknown-column"
    );
    assert_eq!(pending_cell_key(3, "amount"), "3:amount");
    assert_eq!(normalize_clipboard_text("a\r\nb\rc"), "a\nb\nc");
}

#[test]
fn clipboard_source_is_verified_against_the_exact_text() {
    clear_clipboard_source();
    let source = ClipboardSource {
        table_key: "t1".to_owned(),
        column_names: vec!["id".to_owned()],
        column_types: vec!["number".to_owned()],
    };
    record_clipboard_source(source.clone(), Some("a\r\nb"));
    assert_eq!(peek_clipboard_source(), Some(source.clone()));
    assert_eq!(source_for_clipboard_text("a\nb"), Some(source.clone()));
    assert_eq!(source_for_clipboard_text("aXb"), None);
    assert_eq!(source_for_clipboard_text(""), None);
    record_clipboard_source(source.clone(), None);
    assert_eq!(source_for_clipboard_text("a\nb"), None);
    clear_clipboard_source();
    assert_eq!(peek_clipboard_source(), None);
}

#[test]
fn row_copy_payload_is_verified_like_the_column_source() {
    clear_row_copy();
    let payload = RowCopyPayload {
        table_key: "t1".to_owned(),
        column_names: vec!["id".to_owned()],
        rows: vec![vec![serde_json::json!(1)]],
    };
    record_row_copy(payload.clone(), "1\r\n2");
    assert_eq!(peek_row_copy(), Some(payload.clone()));
    assert_eq!(row_copy_for_clipboard_text("1\n2"), Some(payload));
    assert_eq!(row_copy_for_clipboard_text("9"), None);
    clear_row_copy();
    assert_eq!(peek_row_copy(), None);
}

#[test]
fn table_key_folds_case_space_and_nullish_values() {
    assert_eq!(table_change_key(None), "");
    let base = context();
    let folded = TableChangeContext {
        connection_id: " CONN ".to_owned(),
        database: String::new(),
        schema: Some("".to_owned()),
        table: " Customers ".to_owned(),
        ..base.clone()
    };
    assert_eq!(
        table_change_key(Some(&folded)),
        r#"["conn","","","customers"]"#
    );
    assert_eq!(
        table_change_key(Some(&base)),
        r#"["conn","shop","public","customers"]"#
    );
    let other = TableChangeContext {
        table: "orders".to_owned(),
        ..base
    };
    assert_ne!(
        table_change_key(Some(&other)),
        r#"["conn","shop","public","customers"]"#
    );
}

#[test]
fn writability_only_blocks_unknown_and_auto_increment_columns() {
    assert_eq!(
        column_writability(None).reason,
        Some(ColumnWritabilityReason::UnknownColumn)
    );
    assert!(!column_writability(None).writable);
    let auto = schema("id", false, true);
    assert_eq!(
        column_writability(Some(&auto)).reason,
        Some(ColumnWritabilityReason::AutoIncrement)
    );
    let normal = schema("name", true, false);
    assert!(column_writability(Some(&normal)).writable);
    assert_eq!(column_writability(Some(&normal)).reason, None);
}

#[test]
fn empty_and_oversized_blocks_are_refused_before_alignment() {
    let empty = plan_paste(&options(mx(&[])));
    assert_eq!(empty.problems[0].code, PasteProblemCode::Empty);
    assert_eq!(empty.alignment, PasteAlignment::Unknown);
    assert!(empty.requires_confirmation);
    assert!(empty.writes.is_empty());
    assert!(empty.bounds.is_none());

    let mut huge = options(mx(&[&["x"]]));
    huge.matrix.byte_length = crate::GRID_PASTE_MAX_BYTES + 1;
    let plan = plan_paste(&huge);
    assert_eq!(plan.problems[0].code, PasteProblemCode::TooLarge);
    assert_eq!(plan.problems[0].detail.as_deref(), Some("1"));
}

#[test]
fn by_name_alignment_drops_unknown_source_columns() {
    let mut opts = options(mx(&[&["1", "alice", "extra"]]));
    opts.source = Some(ClipboardSource {
        table_key: "src".to_owned(),
        column_names: vec!["id".to_owned(), "name".to_owned(), "gone".to_owned()],
        column_types: vec![],
    });
    let plan = plan_paste(&opts);
    assert_eq!(plan.alignment, PasteAlignment::ByName);
    assert_eq!(plan.matched_columns, 2);
    assert_eq!(plan.dropped_columns, ["gone"]);
    assert_eq!(plan.writes.len(), 2);
    assert_eq!(plan.writes[0].column_name, "id");
    assert_eq!(plan.writes[1].column_name, "name");
    assert_eq!(plan.problems[0].code, PasteProblemCode::ColumnMismatch);
    assert_eq!(plan.problems[0].detail.as_deref(), Some("gone"));
    assert_eq!(plan.skipped, 1);
}

#[test]
fn by_position_alignment_uses_the_landing_column() {
    let mut opts = options(mx(&[&["alice"]]));
    opts.anchor.column_name = "name".to_owned();
    let plan = plan_paste(&opts);
    assert_eq!(plan.alignment, PasteAlignment::ByPosition);
    assert_eq!(plan.writes.len(), 1);
    assert_eq!(plan.writes[0].column_name, "name");
    assert_eq!(plan.matched_columns, 0);

    let bad = options(mx(&[&["x"]]));
    let mut bad = bad;
    bad.anchor.column_name = "missing".to_owned();
    assert_eq!(
        plan_paste(&bad).problems[0].code,
        PasteProblemCode::NoWritableCell
    );
}

#[test]
fn confirmation_gate_uses_table_identity_not_session() {
    let mut same = options(mx(&[&["1"]]));
    let source = ClipboardSource {
        table_key: table_change_key(Some(&context())),
        column_names: vec!["id".to_owned()],
        column_types: vec![],
    };
    same.source = Some(source.clone());
    same.context = Some(context());
    let plan = plan_paste(&same);
    assert_eq!(plan.cross_table, Some(false));
    assert!(!plan.requires_confirmation);

    let mut different = same;
    different.context = Some(TableChangeContext {
        table: "orders".to_owned(),
        ..context()
    });
    let plan = plan_paste(&different);
    assert_eq!(plan.cross_table, Some(true));
    assert!(plan.requires_confirmation);

    let mut unknown = options(mx(&[&["1"]]));
    unknown.source = Some(source);
    let plan = plan_paste(&unknown);
    assert_eq!(plan.cross_table, None);
    assert!(plan.requires_confirmation);
}

#[test]
fn null_literals_quoted_cells_and_not_null_columns() {
    let mut opts = options(quoted_mx(
        &[&["", "\\N", "NULL", "NULL"]],
        &[&[false, false, false, true]],
    ));
    opts.anchor.column_name = "a".to_owned();
    opts.column_order = vec![
        "a".to_owned(),
        "b".to_owned(),
        "c".to_owned(),
        "d".to_owned(),
    ];
    opts.columns = vec![
        schema("a", true, false),
        schema("b", true, false),
        schema("c", true, false),
        schema("d", true, false),
    ];
    let plan = plan_paste(&opts);
    assert_eq!(plan.writes.len(), 4);
    assert!(plan.writes[..3].iter().all(|write| write.value.is_null()));
    assert_eq!(plan.writes[3].value.as_str(), Some("NULL"));

    let mut strict = options(mx(&[&[""]]));
    strict.columns[0] = schema("id", false, false);
    let plan = plan_paste(&strict);
    assert!(plan.writes.is_empty());
    assert_eq!(plan.problems[0].code, PasteProblemCode::NotNullEmpty);
    assert_eq!(plan.problems[0].row, Some(0));
    assert_eq!(plan.skipped, 1);

    let mut custom = options(mx(&[&["nil"]]));
    custom.columns[0] = schema("id", true, false);
    custom.null_literal = "nil".to_owned();
    assert!(plan_paste(&custom).writes[0].value.is_null());
}

#[test]
fn unwritable_columns_are_reported_once_and_deduplicated() {
    let mut opts = options(mx(&[&["1", "2"], &["3", "4"]]));
    opts.columns[1] = schema("name", true, true);
    let plan = plan_paste(&opts);
    assert_eq!(plan.writes.len(), 2);
    assert_eq!(plan.skipped, 2);
    assert_eq!(plan.skipped_columns, ["name"]);
    let read_only: Vec<_> = plan
        .problems
        .iter()
        .filter(|p| p.code == PasteProblemCode::ReadOnly)
        .collect();
    assert_eq!(read_only.len(), 1);
    assert_eq!(read_only[0].column.as_deref(), Some("name"));

    let mut all_blocked = options(mx(&[&["1"]]));
    all_blocked.columns[0] = schema("id", false, true);
    let plan = plan_paste(&all_blocked);
    assert!(plan.writes.is_empty());
    assert!(plan
        .problems
        .iter()
        .any(|p| p.code == PasteProblemCode::NoWritableCell));
}

#[test]
fn clip_mode_respects_region_and_reports_truncation() {
    let mut opts = options(mx(&[&["1", "a"], &["2", "b"], &["3", "c"]]));
    opts.overflow = PasteOverflow::Clip;
    opts.region = Some(GridClipboardRange {
        top_row: 0,
        bottom_row: 1,
        left_column: "id".to_owned(),
        right_column: "id".to_owned(),
    });
    let plan = plan_paste(&opts);
    assert_eq!(plan.writes.len(), 2);
    assert_eq!(plan.skipped, 4);
    assert!(plan
        .problems
        .iter()
        .any(|p| p.code == PasteProblemCode::OutOfBounds && p.detail.as_deref() == Some("2")));
    assert_eq!(plan.bounds.as_ref().unwrap().bottom_row, 1);
}

#[test]
fn pending_overwrites_and_bounds_are_reported() {
    let mut keys = HashSet::new();
    keys.insert(pending_cell_key(0, "id"));
    keys.insert(pending_cell_key(9, "id"));
    let mut opts = options(mx(&[&["1", "a"], &["2", "b"]]));
    opts.pending_cell_keys = keys;
    let plan = plan_paste(&opts);
    assert_eq!(plan.overwritten_pending, 1);
    let bounds = plan.bounds.unwrap();
    assert_eq!(bounds.top_row, 0);
    assert_eq!(bounds.bottom_row, 1);
    assert_eq!(bounds.left_column, "id");
    assert_eq!(bounds.right_column, "name");
}
