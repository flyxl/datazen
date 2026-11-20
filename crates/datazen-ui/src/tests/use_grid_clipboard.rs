//! T294 audit: the useGridClipboard orchestration, verified against the
//! React source re-read line by line and
//! `hooks/__tests__/useGridClipboard.test.tsx` +
//! `useGridClipboard.rowCopy.test.tsx` + `useGridClipboard.htmlCopy.test.tsx`
//! (every case mirrored, plus the carried pure helpers' own pins — the
//! parser's quoting/ragged rules, the serializer's six formats, the
//! planner's L1/L2/L3 gates, the registries' clipboard-text binding).
//!
//! Tester-written: every expectation below is re-derived from the React
//! files. Each Rust test runs on its own thread, so the thread-local
//! copy-source / row-copy registries start clean per test — the same
//! isolation React's `beforeEach(clearClipboardSource)` bought.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use serde_json::{json, Value};

use crate::hooks::grid_clipboard_helpers::{
    parse_clipboard_matrix, serialize_cells, ClipboardDelimiter, ClipboardFormat,
    ClipboardParseResult, PasteProblem, SerializeCellsOptions, GRID_PASTE_CONFIRM_CELLS,
};
use crate::hooks::grid_paste_plan_helpers::{
    clear_clipboard_source, clear_row_copy, peek_clipboard_source, peek_row_copy,
    record_clipboard_source, record_row_copy, row_copy_for_clipboard_text, table_change_key,
    CellPasteWrite, ClipboardSource, PasteCellProblem, PasteOverflow, StagePasteWritesResult,
};
use crate::hooks::use_clipboard_copy_feedback::ClipboardCopyWrite;
use crate::hooks::use_grid_clipboard::{
    describe_paste_problem, rows_to_draft_values, GridClipboardEnv, GridClipboardHook,
    GridCopyTarget, RowsPasteStart,
};
use crate::table_data_connection_state::{
    CellCoord, ColumnSchema, GridSelection, GridSelectionMode, TableChangeContext,
};

// ---- fixtures (the React suite's, verbatim) ---------------------------------

fn columns() -> Vec<ColumnSchema> {
    vec![
        ColumnSchema {
            name: "id".to_owned(),
            data_type: "integer".to_owned(),
            nullable: false,
            default_value: None,
            is_primary_key: true,
            is_auto_increment: false,
            comment: None,
        },
        ColumnSchema {
            name: "name".to_owned(),
            data_type: "text".to_owned(),
            nullable: true,
            default_value: None,
            is_primary_key: false,
            is_auto_increment: false,
            comment: None,
        },
    ]
}

fn row_columns() -> Vec<ColumnSchema> {
    // The row-copy suite's variant: `id` is ALSO auto-increment.
    let mut cols = columns();
    cols[0].is_auto_increment = true;
    cols
}

fn column_order() -> Vec<String> {
    vec!["id".to_owned(), "name".to_owned()]
}

fn context() -> TableChangeContext {
    TableChangeContext {
        connection_id: Some("conn-1".to_owned()),
        db_session_id: "sess-1".to_owned(),
        driver_type: Some("postgres".to_owned()),
        database: Some("app".to_owned()),
        schema: None,
        table: "users".to_owned(),
    }
}

fn row_context() -> TableChangeContext {
    TableChangeContext {
        connection_id: Some("c".to_owned()),
        db_session_id: "s1".to_owned(),
        driver_type: Some("postgresql".to_owned()),
        database: Some("db".to_owned()),
        schema: None,
        table: "t".to_owned(),
    }
}

fn make_rows(count: usize) -> Vec<HashMap<String, Value>> {
    (0..count)
        .map(|index| {
            HashMap::from([
                ("id".to_owned(), json!(index + 1)),
                ("name".to_owned(), json!(format!("n{index}"))),
            ])
        })
        .collect()
}

fn cell_selection(top: usize, bottom: usize, left: &str, right: &str) -> GridSelection {
    GridSelection {
        anchor: Some(CellCoord { row_index: top, column_name: left.to_owned() }),
        focus: Some(CellCoord { row_index: bottom, column_name: right.to_owned() }),
        extra_ranges: Vec::new(),
        mode: GridSelectionMode::Cell,
    }
}

/// The default env: 3 rows, a `name` single-cell selection, staging that
/// records every call (the React `staged.push([...writes])`). The
/// closures live in the harness so the returned env can borrow them —
/// an env must not outlive its actions.
struct Harness {
    staged: Rc<RefCell<Vec<Vec<CellPasteWrite>>>>,
    read_only_tip: Rc<RefCell<usize>>,
    on_problems: Rc<RefCell<Vec<&'static str>>>,
    drafts: Rc<RefCell<Vec<Vec<HashMap<String, Option<Value>>>>>>,
    stage_fn: Box<dyn Fn(&[CellPasteWrite]) -> StagePasteWritesResult>,
    problem_fn: Box<dyn Fn(&PasteProblem)>,
    tip_fn: Box<dyn Fn()>,
    draft_fn: Box<dyn Fn(&[HashMap<String, Option<Value>>]) -> Vec<String>>,
    /// The empty set `env()` borrows (a temporary cannot outlive the env).
    no_selected: HashSet<usize>,
    /// The row-copy suite's column variant (`id` also auto-increment).
    row_cols: Vec<ColumnSchema>,
    /// Owned fixtures the env borrows: render order, empty pending
    /// changes, empty anchors.
    order: Vec<String>,
    no_pending: HashMap<String, crate::table_data_connection_state::PendingRowChange>,
    no_anchors: HashMap<usize, String>,
}

impl Harness {
    fn new() -> Self {
        let staged = Rc::new(RefCell::new(Vec::new()));
        let staged_capture = staged.clone();
        let tip = Rc::new(RefCell::new(0usize));
        let tip_capture = tip.clone();
        let problems = Rc::new(RefCell::new(Vec::new()));
        let problems_capture = problems.clone();
        let drafts = Rc::new(RefCell::new(Vec::new()));
        let drafts_capture = drafts.clone();
        Self {
            staged,
            read_only_tip: tip,
            on_problems: problems,
            drafts,
            stage_fn: Box::new(move |writes: &[CellPasteWrite]| {
                staged_capture.borrow_mut().push(writes.to_vec());
                StagePasteWritesResult { staged: writes.len(), skipped: 0, problems: Vec::new() }
            }),
            problem_fn: Box::new(move |problem: &PasteProblem| {
                problems_capture.borrow_mut().push(problem.code);
            }),
            tip_fn: Box::new(move || {
                *tip_capture.borrow_mut() += 1;
            }),
            draft_fn: Box::new(move |rows: &[HashMap<String, Option<Value>>]| {
                drafts_capture.borrow_mut().push(rows.to_vec());
                Vec::new()
            }),
            no_selected: HashSet::new(),
            row_cols: row_columns(),
            order: column_order(),
            no_pending: HashMap::new(),
            no_anchors: HashMap::new(),
        }
    }

    fn env<'a>(
        &'a self,
        rows: &'a [HashMap<String, Value>],
        columns: &'a [ColumnSchema],
        grid_selection: &'a GridSelection,
        context: Option<&'a TableChangeContext>,
        is_editable: bool,
        with_drafts: bool,
    ) -> GridClipboardEnv<'a> {
        self.env_with(
            rows,
            columns,
            grid_selection,
            &self.no_selected,
            context,
            "users",
            is_editable,
            with_drafts,
        )
    }

    /// The fully-parameterised env builder; every closure comes from
    /// the harness so the returned env borrows something long-lived.
    #[allow(clippy::too_many_arguments)]
    fn env_with<'a>(
        &'a self,
        rows: &'a [HashMap<String, Value>],
        columns: &'a [ColumnSchema],
        grid_selection: &'a GridSelection,
        selected: &'a HashSet<usize>,
        context: Option<&'a TableChangeContext>,
        table_label: &'a str,
        is_editable: bool,
        with_drafts: bool,
    ) -> GridClipboardEnv<'a> {
        GridClipboardEnv {
            is_editable,
            columns,
            column_order: &self.order,
            rows,
            pending_changes: &self.no_pending,
            row_identity_anchors: &self.no_anchors,
            selected_rows: selected,
            grid_selection,
            context,
            table_label,
            overflow: PasteOverflow::Expand,
            stage_paste_writes: &*self.stage_fn,
            on_problem: Some(&*self.problem_fn),
            show_read_only_tip: Some(&*self.tip_fn),
            add_draft_rows: with_drafts.then(|| &*self.draft_fn),
            now_ms: 0,
        }
    }
}

/// `pretendCopied`: record that **this exact text** came from the table
/// under test — the record is only usable while the clipboard still
/// holds it.
fn pretend_copied(text: &str, order: &[String], types: &[&str]) {
    record_clipboard_source(
        ClipboardSource {
            table_key: table_change_key(Some(&context())),
            column_names: order.to_vec(),
            column_types: types
                .iter()
                .map(|family| match *family {
                    "number" => crate::support::data_type_colors::DataTypeFamily::Number,
                    _ => crate::support::data_type_colors::DataTypeFamily::Text,
                })
                .collect(),
        },
        Some(text),
    );
}

// ---- the carried parse/serialize kernel (lib/gridClipboard.ts) ---------------

#[test]
fn the_parser_is_rfc4180_and_refuses_structurally() {
    // Quoted fields: `""` restored, CRLF inside quotes is one newline,
    // and the quoting survives the round trip.
    let ClipboardParseResult::Ok { matrix, problems } = parse_clipboard_matrix("a\t\"x\"\"y\"\r\nz\t\"l\r\nm\"") else {
        panic!("expected a parsed block");
    };
    assert_eq!(matrix.cells, vec![vec!["a", "x\"y"], vec!["z", "l\nm"]]);
    assert_eq!(matrix.quoted, vec![vec![false, true], vec![false, true]]);
    assert_eq!(matrix.delimiter, ClipboardDelimiter::Tab);
    assert!(matrix.multiline);
    assert_eq!(matrix.byte_length, "a\t\"x\"\"y\"\r\nz\t\"l\r\nm\"".len());
    assert!(problems.is_empty(), "a closed quote set is not a problem");

    // A trailing line break must not fabricate an empty row.
    let ClipboardParseResult::Ok { matrix, .. } = parse_clipboard_matrix("a\n") else {
        panic!("expected one row");
    };
    assert_eq!(matrix.cells, vec![vec!["a"]]);

    // An unclosed quote is not a refusal: the rest becomes one cell and
    // the caller is told.
    let ClipboardParseResult::Ok { matrix, problems } = parse_clipboard_matrix("\"unclosed\ttab") else {
        panic!("unclosed quotes still parse");
    };
    assert_eq!(matrix.cells, vec![vec!["unclosed\ttab"]]);
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, "grid.paste.unclosedQuote");

    // Empty means "nothing here", but unquoted empty fields are a good
    // paste that writes NULLs.
    assert!(matches!(
        parse_clipboard_matrix("   "),
        ClipboardParseResult::Refused { code: "grid.paste.empty", cells: 0, .. }
    ));
    let ClipboardParseResult::Ok { matrix, .. } = parse_clipboard_matrix("\t") else {
        panic!("a lone delimiter is one row of two empty fields");
    };
    assert_eq!(matrix.cells, vec![vec!["", ""]]);

    // Ragged rows refuse with the counts.
    assert!(matches!(
        parse_clipboard_matrix("a\tb\nc"),
        ClipboardParseResult::Refused {
            code: "grid.paste.raggedRow",
            row: Some(1),
            expected: Some(2),
            actual: Some(1),
            ..
        }
    ));

    // Delimiter sniffing outside quotes: tab wins, then comma, then
    // semicolon; none means one column (not a comma).
    assert_eq!(
        crate::hooks::grid_clipboard_helpers::sniff_clipboard_delimiter("a,b\tc"),
        ClipboardDelimiter::Tab
    );
    assert_eq!(
        crate::hooks::grid_clipboard_helpers::sniff_clipboard_delimiter("a,b;c"),
        ClipboardDelimiter::Comma
    );
    assert_eq!(
        crate::hooks::grid_clipboard_helpers::sniff_clipboard_delimiter("a;b"),
        ClipboardDelimiter::Semicolon
    );
    assert_eq!(
        crate::hooks::grid_clipboard_helpers::sniff_clipboard_delimiter("a b"),
        ClipboardDelimiter::Tab
    );
    // A comma inside quotes does not count.
    assert_eq!(
        crate::hooks::grid_clipboard_helpers::sniff_clipboard_delimiter("\"a,b\";c"),
        ClipboardDelimiter::Semicolon
    );
}

#[test]
fn the_one_serializer_speaks_all_six_formats() {
    let rows = vec![
        vec![json!(1), json!("Alice")],
        vec![json!(2), json!(Bo_with_lt())],
    ];
    let names = column_order();

    // TSV: no quoting needed.
    assert_eq!(
        serialize_cells(&rows, &SerializeCellsOptions {
            format: Some(ClipboardFormat::Tsv),
            column_names: &names,
            ..SerializeCellsOptions::default()
        }),
        "1\tAlice\n2\tBo<b>"
    );
    // A delimiter/quote/newline inside a value quotes the field and
    // doubles inner quotes (defect G2's fix).
    let tricky = vec![vec![json!("a\tb"), json!("say \"hi\""), Value::Null]];
    assert_eq!(
        serialize_cells(&tricky, &SerializeCellsOptions {
            format: Some(ClipboardFormat::Tsv),
            column_names: &[],
            ..SerializeCellsOptions::default()
        }),
        "\"a\tb\"\t\"say \"\"hi\"\"\"\t"
    );
    // CSV defaults to a header; TSV does not (Excel alignment).
    assert_eq!(
        serialize_cells(&rows, &SerializeCellsOptions {
            format: Some(ClipboardFormat::Csv),
            column_names: &names,
            ..SerializeCellsOptions::default()
        }),
        "id,name\n1,Alice\n2,Bo<b>"
    );
    // Null literal keeps NULL distinguishable from ''.
    assert_eq!(
        serialize_cells(&tricky, &SerializeCellsOptions {
            format: Some(ClipboardFormat::Tsv),
            column_names: &[],
            null_literal: Some("NULL"),
            ..SerializeCellsOptions::default()
        }),
        "\"a\tb\"\t\"say \"\"hi\"\"\"\tNULL"
    );
    // HTML: header always, text nodes escaped (&, <, > — not quotes).
    let html = serialize_cells(&rows, &SerializeCellsOptions {
        format: Some(ClipboardFormat::Html),
        column_names: &names,
        ..SerializeCellsOptions::default()
    });
    assert!(html.contains("<thead><tr><th>id</th><th>name</th></tr></thead>"));
    assert!(html.contains("<td>Bo&lt;b&gt;</td>"));
    assert!(!html.contains("Bo<b>"));
    // Markdown: header + separator, pipes escaped, newlines to <br>.
    let markdown = serialize_cells(&rows, &SerializeCellsOptions {
        format: Some(ClipboardFormat::Markdown),
        column_names: &names,
        ..SerializeCellsOptions::default()
    });
    assert!(markdown.contains("| id | name |"));
    assert!(markdown.contains("| 1 | Alice |"));
    let md_escape = serialize_cells(
        &[vec![json!("a|b\nc"), json!("\\|x")]],
        &SerializeCellsOptions {
            format: Some(ClipboardFormat::Markdown),
            column_names: &[],
            ..SerializeCellsOptions::default()
        },
    );
    assert!(md_escape.contains("a\\|b<br>c"));
    assert!(md_escape.contains("\\\\\\|x"));
    // Plain: no quoting at all.
    assert_eq!(
        serialize_cells(&rows, &SerializeCellsOptions {
            format: Some(ClipboardFormat::Plain),
            column_names: &names,
            include_header: Some(true),
            ..SerializeCellsOptions::default()
        }),
        "id\tname\n1\tAlice\n2\tBo<b>"
    );
    // JSON: positional by default, named records on request; an object
    // cell becomes JSON, never `[object Object]`.
    assert_eq!(
        serialize_cells(&rows, &SerializeCellsOptions {
            format: Some(ClipboardFormat::Json),
            column_names: &names,
            ..SerializeCellsOptions::default()
        }),
        "[[1,\"Alice\"],[2,\"Bo<b>\"]]"
    );
    assert_eq!(
        serialize_cells(&rows, &SerializeCellsOptions {
            format: Some(ClipboardFormat::Json),
            column_names: &names,
            json_objects: Some(true),
            ..SerializeCellsOptions::default()
        }),
        "[{\"id\":1,\"name\":\"Alice\"},{\"id\":2,\"name\":\"Bo<b>\"}]"
    );
    let object_cell = serialize_cells(
        &[vec![json!({"b": 1, "a": [1, 2]})]],
        &SerializeCellsOptions {
            format: Some(ClipboardFormat::Tsv),
            column_names: &[],
            ..SerializeCellsOptions::default()
        },
    );
    // A `"` in the value quotes the field and doubles the inner quotes
    // (React's own pin for `{a: 1}`), so a spreadsheet reads back
    // exactly `{"a":[1,2],"b":1}`.
    assert_eq!(object_cell, "\"{\"\"a\"\":[1,2],\"\"b\"\":1}\"");
}

fn Bo_with_lt() -> String {
    "Bo<b>".to_owned()
}

// ---- the copy-source / row-copy registries (lib/gridPastePlan.ts) -----------

#[test]
fn a_source_is_only_trusted_while_the_clipboard_holds_it() {
    clear_clipboard_source();
    record_clipboard_source(
        ClipboardSource {
            table_key: "k".to_owned(),
            column_names: vec!["a".to_owned()],
            column_types: Vec::new(),
        },
        Some("x\ty"),
    );
    assert!(peek_clipboard_source().is_some());
    assert!(crate::hooks::grid_paste_plan_helpers::source_for_clipboard_text("x\ty").is_some());
    // Excel overwrote the clipboard: the source describes something
    // else and must be discarded, not trusted.
    assert!(crate::hooks::grid_paste_plan_helpers::source_for_clipboard_text("from excel").is_none());
    // Windows rewrote the trailing newline of the copy: the record that
    // carries it still verifies through CRLF normalization on both sides.
    record_clipboard_source(
        ClipboardSource {
            table_key: "k".to_owned(),
            column_names: vec!["a".to_owned()],
            column_types: Vec::new(),
        },
        Some("x\ty\n"),
    );
    assert!(crate::hooks::grid_paste_plan_helpers::source_for_clipboard_text("x\ty\r\n").is_some());
    // A recording that could not say the text stops the source from
    // steering anything (failure direction: a confirmation).
    record_clipboard_source(
        ClipboardSource {
            table_key: "k".to_owned(),
            column_names: vec!["a".to_owned()],
            column_types: Vec::new(),
        },
        None,
    );
    assert!(crate::hooks::grid_paste_plan_helpers::source_for_clipboard_text("x\ty").is_none());
    clear_clipboard_source();
    assert!(peek_clipboard_source().is_none());
}

#[test]
fn a_row_copy_round_trips_only_through_its_text() {
    clear_row_copy();
    record_row_copy(
        crate::hooks::grid_paste_plan_helpers::RowCopyPayload {
            table_key: "[\"c\",\"db\",\"\",\"t\"]".to_owned(),
            column_names: column_order(),
            rows: vec![vec![json!(1), json!("a")], vec![json!(2), Value::Null]],
        },
        "1\ta\n2\t",
    );
    let payload = row_copy_for_clipboard_text("1\ta\n2\t").expect("the text matches");
    assert_eq!(
        payload.rows,
        vec![vec![json!(1), json!("a")], vec![json!(2), Value::Null]]
    );
    assert!(peek_row_copy().is_some());
    // A different clipboard text invalidates the payload.
    assert!(row_copy_for_clipboard_text("something else from Excel").is_none());
    // CRLF from the OS clipboard still verifies.
    record_row_copy(
        crate::hooks::grid_paste_plan_helpers::RowCopyPayload {
            table_key: "k".to_owned(),
            column_names: column_order(),
            rows: vec![vec![json!("x"), json!("y")]],
        },
        "x\ty\n",
    );
    assert!(row_copy_for_clipboard_text("x\ty\r\n").is_some());
    clear_row_copy();
    assert!(row_copy_for_clipboard_text("x").is_none());
}

#[test]
fn the_table_key_folds_case_and_treats_null_and_empty_the_same() {
    let mut ctx = context();
    assert_eq!(table_change_key(Some(&ctx)), "[\"conn-1\",\"app\",\"\",\"users\"]");
    // Never dbSessionId (it changes on reconnect).
    ctx.db_session_id = "sess-999".to_owned();
    assert_eq!(table_change_key(Some(&ctx)), "[\"conn-1\",\"app\",\"\",\"users\"]");
    // Case folds; null database == empty database (SQLite).
    ctx.database = Some("APP".to_owned());
    assert_eq!(table_change_key(Some(&ctx)), "[\"conn-1\",\"app\",\"\",\"users\"]");
    ctx.database = None;
    assert_eq!(table_change_key(Some(&ctx)), "[\"conn-1\",\"\",\"\",\"users\"]");
    assert_eq!(table_change_key(None), "");
}

// ---- rowsToDraftValues (DBR-02) ---------------------------------------------

#[test]
fn rows_to_drafts_strip_identity_and_keep_explicit_nulls() {
    let cols = row_columns();
    // Primary-key AND auto-increment columns never travel.
    let drafts = rows_to_draft_values(
        &column_order(),
        &[vec![json!(5), json!("kept")]],
        &cols,
    );
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].get("name"), Some(&Some(json!("kept"))));
    assert!(!drafts[0].contains_key("id"));

    // An explicit NULL is a written key (absent key = don't write).
    let drafts = rows_to_draft_values(
        &["name".to_owned(), "id".to_owned()],
        &[vec![json!("a"), json!(1)], vec![Value::Null, json!(2)]],
        &cols,
    );
    assert_eq!(drafts[0].get("name"), Some(&Some(json!("a"))));
    assert_eq!(drafts[1].get("name"), Some(&None));

    // Columns the target does not know are skipped.
    let drafts = rows_to_draft_values(
        &["unknown_col".to_owned(), "name".to_owned()],
        &[vec![json!("v"), json!("a")]],
        &cols,
    );
    assert_eq!(drafts[0].get("name"), Some(&Some(json!("a"))));
    assert!(!drafts[0].contains_key("unknown_col"));
}

// ---- describePasteProblem ----------------------------------------------------

#[test]
fn describe_paste_problem_renders_every_code() {
    let problem = |code: &'static str, detail: Option<&str>, column: Option<&str>| PasteProblem {
        code,
        row: Some(2),
        column: column.map(str::to_owned),
        detail: detail.map(str::to_owned),
    };
    assert!(describe_paste_problem(&problem("grid.paste.tooLarge", Some("9000"), None))
        .contains("9000"));
    assert!(describe_paste_problem(&problem("grid.paste.tooLarge", Some("9000"), None))
        .contains("100000"));
    // The row is 1-based in the sentence.
    let not_null = describe_paste_problem(&problem("grid.paste.notNullEmpty", None, Some("name")));
    assert!(not_null.contains("name"));
    // ReadOnly names the column; detail is the reason, never the names.
    let read_only =
        describe_paste_problem(&problem("grid.cell.readOnly", Some("auto-increment"), Some("id")));
    assert!(read_only.contains("id"));
    assert!(!read_only.contains("auto-increment"));
    let whole_panel = describe_paste_problem(&problem("grid.cell.readOnly", None, None));
    assert_eq!(whole_panel, crate::i18n::t("dataTable.readOnlyPasteDisabled"));
    // Unknown codes are shown, never swallowed.
    assert_eq!(
        describe_paste_problem(&problem("grid.paste.brandNew", Some("the detail"), None)),
        "the detail"
    );
    assert_eq!(describe_paste_problem(&problem("grid.paste.brandNew", None, None)), "grid.paste.brandNew");
}

// ---- the paste orchestration (useGridClipboard.test.tsx) ---------------------

#[test]
fn stages_a_pasted_block_and_reports_the_summary() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(3);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    // After a same-table copy the block aligns **by name**: the first
    // field is the source's first column (`id`), whichever cell the
    // user pasted into.
    pretend_copied("7\tAlice", &column_order(), &["number", "text"]);
    assert!(hook.begin_paste(&env, "7\tAlice", &CellCoord { row_index: 0, column_name: "name".to_owned() }));

    assert_eq!(harness.staged.borrow().len(), 1);
    assert_eq!(
        harness.staged.borrow()[0],
        vec![
            CellPasteWrite { row_index: 0, column_name: "id".to_owned(), value: Some(json!("7")) },
            CellPasteWrite { row_index: 0, column_name: "name".to_owned(), value: Some(json!("Alice")) },
        ]
    );
    let last = hook.last_paste().expect("the paste finished");
    assert_eq!(last.staged, 2);
    assert_eq!(last.skipped, 0);
    assert!(!last.cancelled);
}

#[test]
fn splits_a_large_block_into_chunks_and_yields_between_them() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let row_count = 2_601;
    let rows = make_rows(row_count);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    // 2 601 rows × 2 columns = 5 202 cells → 5 000 + 202. A block this
    // size is above the confirmation threshold by definition, so the
    // dialog is part of the journey — chunking happens *after* yes.
    let text = (0..row_count).map(|_| "x\ty".to_owned()).collect::<Vec<_>>().join("\n");
    pretend_copied(&text, &column_order(), &["number", "text"]);
    assert!(hook.begin_paste(&env, &text, &CellCoord { row_index: 0, column_name: "name".to_owned() }));
    assert!(hook.confirm().is_open(), "5 202 cells ask first");
    hook.on_confirm_answer(&env, true);

    // The first pump staged 5 000 and yielded; the second finishes.
    assert!(hook.staging_in_progress());
    let more = hook.pump(&env);
    assert!(!more);
    assert_eq!(
        harness.staged.borrow().iter().map(Vec::len).collect::<Vec<_>>(),
        vec![5_000, 202]
    );
    assert_eq!(hook.last_paste().expect("done").staged, row_count * 2);
}

#[test]
fn asks_first_when_the_block_is_above_the_confirmation_threshold() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows_count = GRID_PASTE_CONFIRM_CELLS + 1;
    let rows = make_rows(rows_count + 1);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    let text = (0..rows_count).map(|_| "x".to_owned()).collect::<Vec<_>>().join("\n");
    assert!(hook.begin_paste(&env, &text, &CellCoord { row_index: 0, column_name: "name".to_owned() }));

    // Nothing is written before the user answers.
    assert!(harness.staged.borrow().is_empty());
    assert!(hook.confirm().is_open());
    hook.on_confirm_answer(&env, true);
    assert_eq!(harness.staged.borrow().len(), 1);
    assert!(hook.last_paste().expect("done").staged > 0);
}

#[test]
fn treats_a_clipboard_that_changed_since_our_copy_as_an_external_paste() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(3);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    // Copy a row here (records the same-table source for THAT text)…
    assert!(hook
        .build_copy_text(&env, &GridCopyTarget::Row { row_indices: vec![0] })
        .is_some());
    // …then paste different text, as if Excel overwrote the clipboard.
    assert!(hook.begin_paste(&env, "x\ty", &CellCoord { row_index: 0, column_name: "name".to_owned() }));

    // It has to ask, exactly as it would for a paste from Excel.
    assert!(harness.staged.borrow().is_empty());
    assert!(hook.confirm().is_open());
    hook.on_confirm_answer(&env, true);
    assert_eq!(harness.staged.borrow().len(), 1);
    // And it lands **by position** from the cell the user aimed at, not
    // by the stale record's column names (which would start at `id`).
    assert_eq!(
        harness.staged.borrow()[0][0],
        CellPasteWrite { row_index: 0, column_name: "name".to_owned(), value: Some(json!("x")) }
    );
}

#[test]
fn writes_nothing_when_the_user_cancels_the_confirmation() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(5);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    // Nothing was copied in this session, so the source is unknown
    // (`crossTable === null`), which is exactly the case that must ask
    // before writing (L3).
    assert!(hook.begin_paste(&env, "x\ty", &CellCoord { row_index: 0, column_name: "name".to_owned() }));
    assert!(hook.confirm().is_open());
    hook.on_confirm_answer(&env, false);

    assert!(harness.staged.borrow().is_empty());
    let last = hook.last_paste().expect("the cancel is a summary too");
    assert_eq!(last.staged, 0);
    assert!(last.cancelled);
}

#[test]
fn claims_nothing_when_the_user_cancels_a_cross_table_paste() {
    // The cross-table notice reads "Pasted from another table into X" —
    // a past-tense claim; cancelling must not leave it behind.
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(3);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    assert!(hook.begin_paste(&env, "x\ty", &CellCoord { row_index: 0, column_name: "name".to_owned() }));
    hook.on_confirm_answer(&env, false);

    assert!(hook.problems().is_empty());
    assert!(hook.last_paste().is_some_and(|last| last.cancelled));
}

#[test]
fn refuses_a_read_only_panel_with_the_tip_and_consumes_the_event() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(3);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), false, false);

    let consumed = hook.begin_paste(&env, "x", &CellCoord { row_index: 0, column_name: "name".to_owned() });
    assert!(consumed);
    assert_eq!(*harness.read_only_tip.borrow(), 1);
    assert!(harness.staged.borrow().is_empty());
    assert!(hook.problems().iter().any(|problem| problem.code == "grid.cell.readOnly"));
}

#[test]
fn reports_a_clipboard_read_failure_instead_of_guessing() {
    // The host's read failed (denied permission, missing binding): the
    // paste is reported, never worked around (B23).
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(3);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);
    hook.on_clipboard_read_failed(&env);

    assert!(harness.staged.borrow().is_empty());
    assert!(hook.problems().iter().any(|problem| problem.code == "grid.clipboard.readFailed"));
}

#[test]
fn marks_the_cells_it_could_not_write() {
    // Staging reports per-cell problems: the hook turns them into
    // `row:column` marks plus banner problems.
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(3);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let stage = move |writes: &[CellPasteWrite]| StagePasteWritesResult {
        staged: writes.len() - 1,
        skipped: 1,
        problems: vec![PasteCellProblem {
            row_index: 0,
            column_name: "id".to_owned(),
            code: "grid.paste.identityChanged",
            detail: None,
        }],
    };
    let ctx = context();
    let env = GridClipboardEnv {
        is_editable: true,
        columns: &cols,
        column_order: &column_order(),
        rows: &rows,
        pending_changes: &HashMap::new(),
        row_identity_anchors: &HashMap::new(),
        selected_rows: &HashSet::new(),
        grid_selection: &selection,
        context: Some(&ctx),
        table_label: "users",
        overflow: PasteOverflow::Expand,
        stage_paste_writes: &stage,
        on_problem: None,
        show_read_only_tip: None,
        add_draft_rows: None,
        now_ms: 0,
    };

    pretend_copied("x\ty", &column_order(), &["number", "text"]);
    assert!(hook.begin_paste(&env, "x\ty", &CellCoord { row_index: 0, column_name: "name".to_owned() }));

    assert_eq!(hook.cell_errors().iter().cloned().collect::<Vec<_>>(), vec!["0:id".to_owned()]);
    assert!(hook.problems().iter().any(|problem| problem.code == "grid.paste.identityChanged"));
}

#[test]
fn builds_the_copy_text_from_the_selection_and_records_where_it_came_from() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(4);
    let cols = columns();
    let selection = cell_selection(0, 1, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    let text = hook.build_copy_text(
        &env,
        &GridCopyTarget::Cell {
            range: crate::hooks::grid_clipboard_helpers::GridClipboardRange {
                top_row: 0,
                bottom_row: 1,
                left_column: "name".to_owned(),
                right_column: "name".to_owned(),
            },
        },
    );
    assert_eq!(text.as_deref(), Some("n0\nn1"));
    // The record is what lets a later paste prove "same table" (L1)…
    let source = peek_clipboard_source().expect("recorded");
    assert_eq!(source.column_names, vec!["name".to_owned()]);
    // …and it is only written when text actually reached the clipboard.
    assert_eq!(source.table_key, "[\"conn-1\",\"app\",\"\",\"users\"]");
}

#[test]
fn does_not_record_a_source_for_an_empty_selection() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(1);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    let text = hook.build_copy_text(&env, &GridCopyTarget::Row { row_indices: Vec::new() });
    assert!(text.is_none());
    assert!(peek_clipboard_source().is_none());
}

#[test]
fn reports_a_dropped_source_column_once_with_its_name() {
    // The clipboard still holds what we copied, so no confirmation — but
    // the block carries a column this table does not have. The planner
    // aggregates the dropped names into one `columnMismatch` with the
    // names in `detail`; the hook reports it once, after the paste.
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(3);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    record_clipboard_source(
        ClipboardSource {
            table_key: table_change_key(Some(&context())),
            column_names: vec!["name".to_owned(), "ghost".to_owned()],
            column_types: Vec::new(),
        },
        Some("Al\tX"),
    );
    assert!(hook.begin_paste(&env, "Al\tX", &CellCoord { row_index: 0, column_name: "name".to_owned() }));

    let dropped: Vec<&PasteProblem> =
        hook.problems().iter().filter(|problem| problem.code == "grid.paste.columnMismatch").collect();
    assert_eq!(dropped.len(), 1);
    // The detail carries the names; without it the sentence names
    // nothing and counts zero.
    assert_eq!(dropped[0].detail.as_deref(), Some("ghost"));
}

#[test]
fn clears_the_per_cell_marks_when_a_new_paste_starts() {
    // Staging reports every cell out of bounds: the marks stay until
    // clearPasteResult (or the next paste) wipes them.
    let hook = &mut GridClipboardHook::new();
    let rows = make_rows(3);
    let cols = columns();
    let selection = cell_selection(0, 0, "name", "name");
    let stage = move |writes: &[CellPasteWrite]| StagePasteWritesResult {
        staged: 0,
        skipped: writes.len(),
        problems: writes
            .iter()
            .map(|write| PasteCellProblem {
                row_index: write.row_index,
                column_name: write.column_name.clone(),
                code: "grid.paste.outOfBounds",
                detail: None,
            })
            .collect(),
    };
    let ctx = context();
    let env = GridClipboardEnv {
        is_editable: true,
        columns: &cols,
        column_order: &column_order(),
        rows: &rows,
        pending_changes: &HashMap::new(),
        row_identity_anchors: &HashMap::new(),
        selected_rows: &HashSet::new(),
        grid_selection: &selection,
        context: Some(&ctx),
        table_label: "users",
        overflow: PasteOverflow::Expand,
        stage_paste_writes: &stage,
        on_problem: None,
        show_read_only_tip: None,
        add_draft_rows: None,
        now_ms: 0,
    };

    pretend_copied("x\ty", &column_order(), &["number", "text"]);
    assert!(hook.begin_paste(&env, "x\ty", &CellCoord { row_index: 0, column_name: "name".to_owned() }));
    assert_eq!(hook.cell_errors().len(), 2);

    hook.clear_paste_result();
    assert_eq!(hook.cell_errors().len(), 0);
    assert!(hook.last_paste().is_none());
}

// ---- the html/markdown copy flavors (useGridClipboard.htmlCopy.test.tsx) ----

#[test]
fn html_copy_hands_the_clipboard_both_flavors() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let cols = columns();
    let rows: Vec<HashMap<String, Value>> = vec![
        HashMap::from([("id".to_owned(), json!(1)), ("name".to_owned(), json!("Alice"))]),
        HashMap::from([("id".to_owned(), json!(2)), ("name".to_owned(), json!("Bo<b>"))]),
    ];
    // anchor (0,'id') → focus (1,'name'): a 2×2 rectangle.
    let selection = cell_selection(0, 1, "id", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    let Some((_, write)) = hook.copy_selected_cells_as(&env, ClipboardFormat::Html, None) else {
        panic!("the copy armed");
    };
    match write {
        ClipboardCopyWrite::Rich { html, text } => {
            // The plain flavor is the TSV of the same rectangle, not the
            // markup.
            assert_eq!(text, "1\tAlice\n2\tBo<b>");
            // The rich flavor is the table, header included,
            // HTML-escaped.
            assert!(html.contains("<table>"));
            assert!(html.contains("<th>id</th><th>name</th>"));
            assert!(html.contains("<td>Bo&lt;b&gt;</td>"));
            assert!(!html.contains("Bo<b>"));
        }
        ClipboardCopyWrite::Plain { .. } => panic!("html takes the two-flavor write"),
    }
}

#[test]
fn other_formats_keep_the_single_text_write() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let cols = columns();
    let rows: Vec<HashMap<String, Value>> = vec![
        HashMap::from([("id".to_owned(), json!(1)), ("name".to_owned(), json!("Alice"))]),
        HashMap::from([("id".to_owned(), json!(2)), ("name".to_owned(), json!("Bo<b>"))]),
    ];
    let selection = cell_selection(0, 1, "id", "name");
    let ctx = context();
    let env = harness.env(&rows, &cols, &selection, Some(&ctx), true, false);

    let Some((_, write)) = hook.copy_selected_cells_as(&env, ClipboardFormat::Markdown, None) else {
        panic!("the copy armed");
    };
    match write {
        ClipboardCopyWrite::Plain { text } => {
            assert!(text.contains("| id | name |"));
            assert!(text.contains("| 1 | Alice |"));
        }
        ClipboardCopyWrite::Rich { .. } => panic!("only html takes the rich write"),
    }
}

// ---- the row features (DBR-02, useGridClipboard.rowCopy.test.tsx) ------------

/// The row-mode selection (built once per test so the env can borrow it).
fn row_selection() -> GridSelection {
    GridSelection {
        anchor: None,
        focus: None,
        extra_ranges: Vec::new(),
        mode: GridSelectionMode::Row,
    }
}

fn row_harness_env<'a>(
    harness: &'a Harness,
    rows: &'a [HashMap<String, Value>],
    selected: &'a HashSet<usize>,
    selection: &'a GridSelection,
    context: Option<&'a TableChangeContext>,
    table_label: &'a str,
) -> GridClipboardEnv<'a> {
    harness.env_with(rows, &harness.row_cols, selection, selected, context, table_label, true, true)
}

fn row_copy_rows() -> Vec<HashMap<String, Value>> {
    vec![
        HashMap::from([("id".to_owned(), json!(1)), ("name".to_owned(), json!("alpha"))]),
        HashMap::from([("id".to_owned(), json!(2)), ("name".to_owned(), Value::Null)]),
        HashMap::from([("id".to_owned(), json!(3)), ("name".to_owned(), json!("gamma"))]),
    ]
}

#[test]
fn copy_rows_as_drafts_copies_the_covered_rows_straight_into_drafts() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = row_copy_rows();
    let selected = HashSet::from([0usize, 1usize]);
    let selection = row_selection();
    let ctx = row_context();
    let env =
        row_harness_env(&harness, &rows, &selected, &selection, Some(&ctx), "t");

    assert!(hook.copy_rows_as_drafts(&env));
    // Two drafts, PK stripped, the explicit NULL in row 2 preserved.
    let drafts = harness.drafts.borrow();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].len(), 2);
    assert_eq!(drafts[0][0].get("name"), Some(&Some(json!("alpha"))));
    assert!(!drafts[0][0].contains_key("id"));
    assert_eq!(drafts[0][1].get("name"), Some(&None));
}

#[test]
fn copy_rows_as_new_rows_writes_the_row_tsv_and_records_the_payload() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = row_copy_rows();
    let selected = HashSet::from([0usize, 1usize]);
    let selection = row_selection();
    let ctx = row_context();
    let env =
        row_harness_env(&harness, &rows, &selected, &selection, Some(&ctx), "t");

    let Some((_, write)) = hook.copy_rows_as_new_rows(&env) else {
        panic!("the copy armed");
    };
    let ClipboardCopyWrite::Plain { text } = write else {
        panic!("row copy is a plain write");
    };
    assert_eq!(text, "1\talpha\n2\t");
    let payload = peek_row_copy().expect("the payload is recorded");
    assert_eq!(payload.table_key, "[\"c\",\"db\",\"\",\"t\"]");
    // The payload keeps the full matrix — PK stripping happens when the
    // rows become drafts, not at copy time (the clipboard stays
    // faithful).
    assert_eq!(payload.column_names, column_order());
    assert_eq!(payload.rows, vec![vec![json!(1), json!("alpha")], vec![json!(2), Value::Null]]);
}

#[test]
fn paste_rows_as_drafts_pastes_a_same_table_copy_without_confirmation() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = row_copy_rows();
    let selected = HashSet::from([2usize]);
    let selection = row_selection();
    let ctx = row_context();
    let env =
        row_harness_env(&harness, &rows, &selected, &selection, Some(&ctx), "t");

    // A same-table row copy (the hook's own context is c/db//t).
    record_row_copy(
        crate::hooks::grid_paste_plan_helpers::RowCopyPayload {
            table_key: "[\"c\",\"db\",\"\",\"t\"]".to_owned(),
            column_names: column_order(),
            rows: vec![vec![json!(7), json!("pasted-name")]],
        },
        "7\tpasted-name",
    );
    assert_eq!(hook.begin_paste_rows(&env), RowsPasteStart::NeedClipboard);
    hook.on_rows_clipboard_text(&env, Some("7\tpasted-name"));

    // No confirm dialog ever opened for a same-table paste.
    assert!(!hook.confirm().is_open());
    let drafts = harness.drafts.borrow();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0][0].get("name"), Some(&Some(json!("pasted-name"))));
    assert!(!drafts[0][0].contains_key("id"));
}

#[test]
fn paste_rows_as_drafts_asks_before_a_cross_table_paste_and_refuses_on_cancel() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = row_copy_rows();
    let selected = HashSet::from([2usize]);
    let selection = row_selection();
    let ctx = row_context();
    let env =
        row_harness_env(&harness, &rows, &selected, &selection, Some(&ctx), "t");

    record_row_copy(
        crate::hooks::grid_paste_plan_helpers::RowCopyPayload {
            table_key: "[\"c\",\"other-db\",\"\",\"source-table\"]".to_owned(),
            column_names: vec!["name".to_owned()],
            rows: vec![vec![json!("cross-name")]],
        },
        "cross-name",
    );
    assert_eq!(hook.begin_paste_rows(&env), RowsPasteStart::NeedClipboard);
    hook.on_rows_clipboard_text(&env, Some("cross-name"));

    // The mapping summary dialog is up; nothing is written before the
    // answer.
    assert!(hook.confirm().is_open());
    assert!(harness.drafts.borrow().is_empty());
    hook.on_confirm_answer(&env, false);
    assert!(harness.drafts.borrow().is_empty());
}

#[test]
fn paste_rows_as_drafts_confirms_a_cross_table_paste_on_ok() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = row_copy_rows();
    let selected = HashSet::from([2usize]);
    let selection = row_selection();
    let ctx = row_context();
    let env =
        row_harness_env(&harness, &rows, &selected, &selection, Some(&ctx), "t");

    record_row_copy(
        crate::hooks::grid_paste_plan_helpers::RowCopyPayload {
            table_key: "[\"c\",\"other-db\",\"\",\"source-table\"]".to_owned(),
            column_names: vec!["name".to_owned()],
            rows: vec![vec![json!("cross-name")]],
        },
        "cross-name",
    );
    hook.begin_paste_rows(&env);
    hook.on_rows_clipboard_text(&env, Some("cross-name"));
    assert!(hook.confirm().is_open());
    hook.on_confirm_answer(&env, true);

    let drafts = harness.drafts.borrow();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0][0].get("name"), Some(&Some(json!("cross-name"))));
}

#[test]
fn paste_rows_as_drafts_refuses_a_clipboard_that_holds_no_datazen_rows() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = row_copy_rows();
    let selected = HashSet::from([2usize]);
    let selection = row_selection();
    let ctx = row_context();
    let env =
        row_harness_env(&harness, &rows, &selected, &selection, Some(&ctx), "t");

    assert_eq!(hook.begin_paste_rows(&env), RowsPasteStart::NeedClipboard);
    // "from excel\tdata": no recorded payload matches it.
    hook.on_rows_clipboard_text(&env, Some("from excel\tdata"));
    assert!(harness.drafts.borrow().is_empty());
    assert!(hook.problems().iter().any(|problem| problem.code == "grid.pasteRows.notRowData"));
}

#[test]
fn paste_rows_as_drafts_reports_when_no_copied_column_can_be_written() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = row_copy_rows();
    let selected = HashSet::from([2usize]);
    let selection = row_selection();
    let ctx = row_context();
    let env =
        row_harness_env(&harness, &rows, &selected, &selection, Some(&ctx), "t");

    // Only the PK column was copied: nothing writable.
    record_row_copy(
        crate::hooks::grid_paste_plan_helpers::RowCopyPayload {
            table_key: "[\"c\",\"db\",\"\",\"t\"]".to_owned(),
            column_names: vec!["id".to_owned()],
            rows: vec![vec![json!(7)]],
        },
        "7",
    );
    hook.begin_paste_rows(&env);
    hook.on_rows_clipboard_text(&env, Some("7"));
    assert!(harness.drafts.borrow().is_empty());
    assert!(
        hook.problems().iter().any(|problem| problem.code == "grid.pasteRows.noWritableColumn")
    );
}

#[test]
fn paste_rows_is_not_offered_without_draft_support() {
    let harness = Harness::new();
    let hook = &mut GridClipboardHook::new();
    let rows = row_copy_rows();
    let selected = HashSet::from([2usize]);
    let cols = row_columns();
    let selection = GridSelection {
        anchor: None,
        focus: None,
        extra_ranges: Vec::new(),
        mode: GridSelectionMode::Row,
    };
    let stage = |_writes: &[CellPasteWrite]| StagePasteWritesResult::default();
    let env = GridClipboardEnv {
        is_editable: true,
        columns: &cols,
        column_order: &column_order(),
        rows: &rows,
        pending_changes: &HashMap::new(),
        row_identity_anchors: &HashMap::new(),
        selected_rows: &selected,
        grid_selection: &selection,
        context: Some(&row_context()),
        table_label: "t",
        overflow: PasteOverflow::Expand,
        stage_paste_writes: &|_writes: &[CellPasteWrite]| StagePasteWritesResult::default(),
        on_problem: None,
        show_read_only_tip: None,
        // Result grids omit addDraftRows: "paste rows" is not offered.
        add_draft_rows: None,
        now_ms: 0,
    };
    assert_eq!(hook.begin_paste_rows(&env), RowsPasteStart::NotOffered);
    assert!(!crate::hooks::use_grid_clipboard::GridClipboardHook::can_use_row_copy(&env));
}

// ---- the planner's own gates (lib/gridPastePlan.ts, re-derived) --------------

#[test]
fn the_planner_skips_read_only_columns_and_respects_clip_and_null_rules() {
    use crate::hooks::grid_clipboard_helpers::GridClipboardRange;
    use crate::hooks::grid_paste_plan_helpers::{plan_paste, PastePlanOptions};

    // A one-column block anchored at the last column: every write
    // targets `name` (position alignment runs out of displayed columns
    // to the right), and unquoted NULL / \N write SQL NULL.
    let ClipboardParseResult::Ok { matrix, .. } = parse_clipboard_matrix("a\nNULL\n\\N") else {
        panic!("the block parses");
    };
    let cols = row_columns(); // id is PK + auto-increment → not paste-writable
    let anchor = CellCoord { row_index: 0, column_name: "name".to_owned() };
    let order = column_order();
    let pending: HashSet<String> = HashSet::new();
    let plan = plan_paste(&PastePlanOptions {
        matrix: &matrix,
        anchor: &anchor,
        column_order: &order,
        columns: &cols,
        region: None,
        page_row_count: 10,
        overflow: PasteOverflow::Expand,
        null_literal: "NULL",
        pending_cell_keys: Some(&pending),
        source: None,
        context: Some(&context()),
    });
    assert_eq!(plan.writes.len(), 3);
    assert!(plan.writes.iter().all(|write| write.column_name == "name"));
    assert_eq!(plan.writes[0].value, Some(json!("a")));
    assert_eq!(plan.writes[1].value, None);
    assert_eq!(plan.writes[2].value, None);

    // Clip keeps the paste inside the region: rows beyond bottomRow are
    // clipped with an outOfBounds count.
    let ClipboardParseResult::Ok { matrix, .. } = parse_clipboard_matrix("a\nb\nc\nd") else {
        panic!("the block parses");
    };
    let plan = plan_paste(&PastePlanOptions {
        matrix: &matrix,
        anchor: &anchor,
        column_order: &order,
        columns: &cols,
        region: Some(GridClipboardRange {
            top_row: 0,
            bottom_row: 1,
            left_column: "name".to_owned(),
            right_column: "name".to_owned(),
        }),
        page_row_count: 10,
        overflow: PasteOverflow::Clip,
        null_literal: "NULL",
        pending_cell_keys: Some(&pending),
        source: None,
        context: Some(&context()),
    });
    assert_eq!(plan.writes.len(), 2);
    assert_eq!(plan.skipped, 2);
    assert!(plan.problems.iter().any(|problem| problem.code == "grid.paste.outOfBounds"));

    // A NOT NULL column refuses empty/NULL cells with a per-cell
    // problem, and the rest still lands (one bad cell must not cost the
    // good ones).
    let ClipboardParseResult::Ok { matrix, .. } = parse_clipboard_matrix("\tq\nx\tz") else {
        panic!("the block parses");
    };
    let cols = columns(); // id: nullable=false, PK but NOT auto-increment → writable
    let anchor = CellCoord { row_index: 0, column_name: "id".to_owned() };
    let plan = plan_paste(&PastePlanOptions {
        matrix: &matrix,
        anchor: &anchor,
        column_order: &order,
        columns: &cols,
        region: None,
        page_row_count: 10,
        overflow: PasteOverflow::Expand,
        null_literal: "NULL",
        pending_cell_keys: Some(&pending),
        source: None,
        context: Some(&context()),
    });
    assert_eq!(plan.writes.len(), 3); // q, x, z land; the empty NOT NULL cell is skipped
    assert!(plan.problems.iter().any(|problem| problem.code == "grid.paste.notNullEmpty"));
    assert_eq!(plan.skipped, 1);

    // An auto-increment column is reported read-only with the reason.
    let ClipboardParseResult::Ok { matrix, .. } = parse_clipboard_matrix("1\t2") else {
        panic!("the block parses");
    };
    let cols = row_columns();
    let anchor = CellCoord { row_index: 0, column_name: "id".to_owned() };
    let plan = plan_paste(&PastePlanOptions {
        matrix: &matrix,
        anchor: &anchor,
        column_order: &order,
        columns: &cols,
        region: None,
        page_row_count: 10,
        overflow: PasteOverflow::Expand,
        null_literal: "NULL",
        pending_cell_keys: Some(&pending),
        source: None,
        context: Some(&context()),
    });
    assert!(plan.writes.iter().all(|write| write.column_name == "name"));
    assert_eq!(plan.skipped_columns, vec!["id".to_owned()]);
    let read_only = plan
        .problems
        .iter()
        .find(|problem| problem.code == "grid.cell.readOnly")
        .expect("the reason is reported");
    assert_eq!(read_only.column.as_deref(), Some("id"));
    assert_eq!(read_only.detail.as_deref(), Some("auto-increment"));
}
