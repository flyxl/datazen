//! T185 audit: the useColumnResize port (DB-12), verified against the
//! React sources re-read line by line and the two React tests
//! (`hooks/__tests__/computeColumnWidths.test.ts`,
//! `hooks/__tests__/useColumnResize.persist.test.tsx` — every case
//! mirrored, plus the rows-do-not-recompute memo pin the React suite
//! implies and the positional fallback the JSX documents).
//!
//! Tester-written: every expectation below is re-derived from the React
//! files.

use std::collections::BTreeMap;

use serde_json::json;

use datazen_ui::hooks::use_column_resize::{
    ColumnResizeOptions, ColumnResizeState, ColumnSortSpec, ColumnWidthInput, SORT_BADGE_EXTRA_WIDTH,
    adjust_widths_for_sort, compute_initial_column_widths,
};

const COLUMNS: &[ColumnWidthInput<'static>] = &[
    ColumnWidthInput { name: "id", data_type: Some("integer") },
    ColumnWidthInput { name: "name", data_type: Some("text") },
    ColumnWidthInput { name: "age", data_type: Some("integer") },
];

fn opts<'a>(
    columns: Option<&'a [ColumnWidthInput<'a>]>,
    persisted: Option<&'a BTreeMap<String, f32>>,
) -> ColumnResizeOptions<'a> {
    ColumnResizeOptions {
        columns,
        rows: &[],
        count: columns.map_or(3, |c| c.len()),
        default_width: 150.0,
        min_width: 60.0,
        persisted_widths: persisted,
    }
}

// ---- computeInitialColumnWidths (the React suite, case for case) ------------

#[test]
fn type_defaults_follow_the_react_table() {
    let w = |name: &str, ty: &str| {
        compute_initial_column_widths(&[ColumnWidthInput::new(name, Some(ty))], &[])[0]
    };
    // Narrow: serial-family and booleans.
    assert!(w("id", "bigint") <= 90.0);
    assert!(w("ok", "boolean") <= 90.0);
    assert!(w("qty", "integer") <= 100.0);
    assert!(w("stock", "smallint") <= 100.0);
    // Medium: varchar-family, numeric-family.
    let varchar = w("name", "character varying");
    assert!((120.0..=200.0).contains(&varchar));
    let numeric = w("price", "numeric");
    assert!((100.0..=140.0).contains(&numeric));
    assert!(numeric <= 105.0, "numeric is not excessively wide");
    // Wide: json.
    assert!(w("metadata", "jsonb") >= 240.0);
    // Timestamps sit between.
    let ts = w("created_at", "timestamp with time zone");
    assert!((170.0..=200.0).contains(&ts));
    // Untyped falls back to the default.
    assert_eq!(w("x", ""), 150.0);
}

#[test]
fn data_rows_widen_a_column_past_the_type_default() {
    let cols = [ColumnWidthInput::new("name", Some("character varying"))];
    let rows = vec![vec![json!("a very long product name that needs more space really")]];
    assert!(compute_initial_column_widths(&cols, &rows)[0] > 150.0);
}

#[test]
fn header_reservation_makes_long_names_fit() {
    // "score_column" x 6.5 + 24 padding + 24 sort control + 8 gap = 134.
    let cols = [ColumnWidthInput::new("score_column", Some("integer"))];
    let rows = vec![vec![json!(0)]];
    assert_eq!(compute_initial_column_widths(&cols, &rows)[0], 134.0);
    // A long-named integer column stays compact against short data.
    let cols = [ColumnWidthInput::new("category_id", Some("integer"))];
    let rows = vec![vec![json!(2)], vec![json!(12)], vec![json!(32)]];
    assert!(compute_initial_column_widths(&cols, &rows)[0] <= 130.0);
}

#[test]
fn objects_stringify_and_the_max_clamps() {
    let cols = [ColumnWidthInput::new("data", Some("jsonb"))];
    let rows = vec![vec![json!({"a":1,"b":2,"c":3,"d":4,"e":"a very very long json field that could be unlimited"})]];
    assert!(compute_initial_column_widths(&cols, &rows)[0] <= 400.0);
    // Nulls skip (React's `cell == null`).
    let rows = vec![vec![serde_json::Value::Null]];
    assert!(compute_initial_column_widths(&cols, &rows)[0] >= 240.0);
    // Booleans stringify.
    let cols = [ColumnWidthInput::new("ok", Some("boolean"))];
    let rows = vec![vec![json!(true)]];
    assert!(compute_initial_column_widths(&cols, &rows)[0] <= 90.0 + 26.0);
}

#[test]
fn mixed_types_order_like_react() {
    let cols = [
        ColumnWidthInput::new("id", Some("bigint")),
        ColumnWidthInput::new("name", Some("character varying")),
        ColumnWidthInput::new("price", Some("numeric")),
        ColumnWidthInput::new("stock", Some("integer")),
        ColumnWidthInput::new("metadata", Some("jsonb")),
        ColumnWidthInput::new("created_at", Some("timestamp with time zone")),
    ];
    let widths = compute_initial_column_widths(&cols, &[]);
    assert!(widths[0] < widths[4]);
    assert!(widths[3] <= widths[1]);
}

// ---- adjustWidthsForSort -----------------------------------------------------

#[test]
fn sort_adjustment_only_reserves_for_multi_sort() {
    let cols = [
        ColumnWidthInput::new("id", None),
        ColumnWidthInput::new("name", None),
        ColumnWidthInput::new("price", None),
    ];
    let widths = [90.0, 150.0, 100.0];
    // No sort: unchanged.
    assert_eq!(
        adjust_widths_for_sort(&widths, &cols, &[]),
        vec![90.0, 150.0, 100.0]
    );
    // Single sort: the adaptive control is reused, no change.
    assert_eq!(
        adjust_widths_for_sort(
            &widths,
            &cols,
            &[ColumnSortSpec { column: "name", descending: Some(false) }]
        ),
        vec![90.0, 150.0, 100.0]
    );
    // Multi sort: each sorted column gains the badge width.
    let multi = adjust_widths_for_sort(
        &widths,
        &cols,
        &[
            ColumnSortSpec { column: "name", descending: Some(false) },
            ColumnSortSpec { column: "price", descending: Some(true) },
        ],
    );
    assert_eq!(multi[0], 90.0);
    assert_eq!(multi[1], 150.0 + SORT_BADGE_EXTRA_WIDTH);
    assert_eq!(multi[2], 100.0 + SORT_BADGE_EXTRA_WIDTH);
    // The input array is not mutated (a new vec, not an in-place edit).
    assert_eq!(widths, [90.0, 150.0, 100.0]);
}

// ---- the by-name state machine (DB-12, the persist suite) --------------------

#[test]
fn persisted_widths_seed_by_name() {
    let mut persisted = BTreeMap::new();
    persisted.insert("name".to_owned(), 222.0);
    persisted.insert("age".to_owned(), 333.0);
    let options = opts(Some(COLUMNS), Some(&persisted));
    let mut state = ColumnResizeState::new();
    let widths = state.column_widths(&options);
    assert_eq!(widths[1], 222.0);
    assert_eq!(widths[2], 333.0);
    // A column without a stored width falls back to the computed one.
    assert!(widths[0] <= 400.0);
    assert_ne!(widths[0], 222.0);
}

#[test]
fn a_drag_files_by_name_and_commits_the_whole_map() {
    let options = opts(Some(COLUMNS), None);
    let mut state = ColumnResizeState::new();
    let drag = state.begin_drag(&options, 1, 500.0);
    drag.on_move(&mut state, 560.0);
    let committed = drag
        .on_end(&state, &options)
        .expect("named columns commit");

    let widths = state.column_widths(&options);
    assert!(widths[1] > 150.0, "the dragged column grew");
    // The commit carries every column's effective width, by name.
    let keys: Vec<&str> = committed.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["age", "id", "name"]);
    assert_eq!(committed.get("name"), widths.get(1));
    // The drag floors at minWidth.
    let drag = state.begin_drag(&options, 1, 500.0);
    drag.on_move(&mut state, 0.0);
    let widths = state.column_widths(&options);
    assert_eq!(widths[1], 60.0);
}

#[test]
fn a_reordered_column_keeps_its_dragged_width() {
    let options = opts(Some(COLUMNS), None);
    let mut state = ColumnResizeState::new();
    let drag = state.begin_drag(&options, 1, 500.0);
    drag.on_move(&mut state, 620.0);
    drag.on_end(&state, &options);
    let dragged = state.column_widths(&options)[1];

    // `name` moves to the front: its width follows the name, not the slot.
    let reordered: &[ColumnWidthInput<'static>] = &[
        ColumnWidthInput::new("name", Some("text")),
        ColumnWidthInput::new("id", Some("integer")),
        ColumnWidthInput::new("age", Some("integer")),
    ];
    let next = opts(Some(reordered), None);
    let widths = state.column_widths(&next);
    assert_eq!(widths[0], dragged);
    assert_ne!(widths[1], dragged);
}

#[test]
fn a_persisted_width_survives_a_reorder() {
    let mut persisted = BTreeMap::new();
    persisted.insert("age".to_owned(), 333.0);
    let options = opts(Some(COLUMNS), Some(&persisted));
    let mut state = ColumnResizeState::new();
    let widths = state.column_widths(&options);
    assert_eq!(widths[2], 333.0);

    let reordered: &[ColumnWidthInput<'static>] = &[
        ColumnWidthInput::new("age", Some("integer")),
        ColumnWidthInput::new("name", Some("text")),
        ColumnWidthInput::new("id", Some("integer")),
    ];
    let next = opts(Some(reordered), Some(&persisted));
    let widths = state.column_widths(&next);
    // `age` is now first, and its stored width is with it.
    assert_eq!(widths[0], 333.0);
}

// ---- the memo and the positional fallback ------------------------------------

/// React's useMemo deps: only a column-signature change recomputes the
/// smart widths — new rows keep the widths the columns were first
/// measured with.
#[test]
fn rows_do_not_recompute_the_smart_widths() {
    let cols = [ColumnWidthInput::new("name", Some("character varying"))];
    let short_rows = vec![vec![json!("ab")]];
    let mut options = ColumnResizeOptions {
        columns: Some(&cols),
        rows: &short_rows,
        count: 1,
        default_width: 150.0,
        min_width: 60.0,
        persisted_widths: None,
    };
    let mut state = ColumnResizeState::new();
    let first = state.column_widths(&options);
    // New rows that would demand more width: the memo keeps the first.
    let long_rows = vec![vec![json!("a much much longer value demanding far more width")]];
    options.rows = &long_rows;
    let second = state.column_widths(&options);
    assert_eq!(first, second);
    assert!(first[0] < compute_initial_column_widths(&cols, &long_rows)[0]);
    // A column-signature change (rename) recomputes.
    let renamed = [ColumnWidthInput::new("title", Some("character varying"))];
    options.columns = Some(&renamed);
    let third = state.column_widths(&options);
    assert_ne!(first, third);
}

/// Without column names the drag still works, filed by position — and
/// no commit is offered (nothing stable to key a width by).
#[test]
fn positional_fallback_without_names() {
    let mut options = opts(None, None);
    options.count = 3;
    let mut state = ColumnResizeState::new();
    let widths = state.column_widths(&options);
    assert_eq!(widths, vec![150.0, 150.0, 150.0]);

    let drag = state.begin_drag(&options, 2, 100.0);
    drag.on_move(&mut state, 220.0);
    let widths = state.column_widths(&options);
    assert_eq!(widths[2], 270.0);
    assert_eq!(widths[0], 150.0, "only the dragged position moved");
    assert!(drag.on_end(&state, &options).is_none(), "no names, no commit");
    // Positional widths do not survive a reorder (the documented limit).
    let _ = options;
}
