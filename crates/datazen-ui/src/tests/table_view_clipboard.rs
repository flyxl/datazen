//! T630 audit for the pure TableView clipboard decisions: copy/paste intent,
//! the single serializer and parser, row/value mapping, permission gates, the
//! paste gate and the host-clock feedback state. No clipboard IPC is faked.

use crate::i18n::t;
use crate::views::table_clipboard_status::PasteProblem;
use crate::views::table_view_clipboard as clip;
use crate::views::table_view_clipboard::state as st;
use crate::views::table_view_clipboard::{
    parse_clipboard_text, serialize_cells, ClipValue, ClipboardColumn, ClipboardFormat,
    ClipboardRow, CopyTarget, ParseResult, PasteDecision, PastePlanInput,
};

fn text(value: &str) -> ClipValue {
    ClipValue::Text(value.to_owned())
}

fn column(name: &str, pk: bool, auto: bool) -> ClipboardColumn {
    ClipboardColumn {
        name: name.to_owned(),
        data_type: "text".to_owned(),
        is_primary_key: pk,
        is_auto_increment: auto,
    }
}

fn row(pairs: &[(&str, ClipValue)]) -> ClipboardRow {
    pairs.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect()
}

fn range() -> clip::CellRange {
    clip::CellRange {
        top_row: 0,
        bottom_row: 2,
        left_column: "a".to_owned(),
        right_column: "b".to_owned(),
    }
}

fn plan(writes: usize) -> PastePlanInput {
    PastePlanInput {
        writes,
        skipped: 7,
        overwritten_pending: 3,
        bounds: Some(range()),
        matched_columns: 2,
        total_columns: 3,
        columns: 2,
        source_key: "other".to_owned(),
        ..PastePlanInput::default()
    }
}

// ── copy / paste intent ───────────────────────────────────────────────────

#[test]
fn copy_target_prefers_cell_selection_then_sorted_rows() {
    let target = clip::copy_target(Some(&range()), &[3, 1]).expect("cell target");
    assert_eq!(
        target,
        CopyTarget::Cell {
            top_row: 0,
            bottom_row: 2,
            left_column: "a".to_owned(),
            right_column: "b".to_owned(),
        }
    );
    assert_eq!(
        clip::copy_target(None, &[4, 1, 2]),
        Some(CopyTarget::Row { row_indices: vec![1, 2, 4] })
    );
    assert_eq!(clip::copy_target(None, &[]), None);
}

#[test]
fn permission_gates_follow_editable_selection_and_draft_capability() {
    assert!(!clip::can_copy(None, &[]));
    assert!(clip::can_copy(Some(&range()), &[]));
    assert!(clip::can_copy(None, &[0]));
    assert!(!clip::can_paste(false, Some(&range())));
    assert!(!clip::can_paste(true, None));
    assert!(clip::can_paste(true, Some(&range())));
    assert!(!clip::can_use_row_copy(false, true));
    assert!(!clip::can_use_row_copy(true, false));
    assert!(clip::can_use_row_copy(true, true));
}

#[test]
fn paste_gate_runs_read_only_parse_no_writes_confirm_then_stage() {
    let ok = parse_clipboard_text("a\tb");
    assert_eq!(clip::decide_paste(false, &ok, &plan(4)), PasteDecision::ReadOnly);
    assert_eq!(
        clip::decide_paste(true, &parse_clipboard_text(""), &plan(4)),
        PasteDecision::ParseRefused
    );

    let none = clip::decide_paste(true, &ok, &plan(0));
    assert!(matches!(none, PasteDecision::NoWrites(ref s) if s.skipped == 7 && s.cells == 0));
    assert_eq!(clip::decide_paste(true, &ok, &plan(2_001)), PasteDecision::NeedConfirm);
    let mut forced = plan(1);
    forced.requires_confirmation = true;
    assert_eq!(clip::decide_paste(true, &ok, &forced), PasteDecision::NeedConfirm);
    assert_eq!(clip::decide_paste(true, &ok, &plan(4)), PasteDecision::Stage { chunks: 1 });
    // Anything above the confirm threshold asks first; chunking is a staging detail.
    assert_eq!(clip::decide_paste(true, &ok, &plan(5_001)), PasteDecision::NeedConfirm);
    assert_eq!(st::stage_chunks(5_001), 2);
    assert_eq!(st::stage_chunks(5_000), 1);
}

#[test]
fn paste_summaries_carry_cells_staged_skipped_and_bounds() {
    let p = plan(9);
    let staged = clip::staged_summary(&p, 7, 11);
    assert_eq!((staged.cells, staged.staged, staged.skipped), (9, 7, 11));
    assert_eq!((staged.rows, staged.columns, staged.overwritten_pending), (3, 2, 3));
    assert!(!staged.cancelled);
    let cancelled = clip::cancelled_summary(&p);
    assert_eq!((cancelled.staged, cancelled.cells), (0, 9));
    assert!(cancelled.cancelled);
}

// ── formatting ────────────────────────────────────────────────────────────

#[test]
fn serializer_quotes_delimiter_quote_and_newline() {
    let names = vec!["x".to_owned(), "y".to_owned()];
    let one = vec!["x".to_owned()];
    assert_eq!(
        serialize_cells(&[vec![text("a\tb"), text("c")]], ClipboardFormat::Tsv, &names, None, "", false),
        "\"a\tb\"\tc"
    );
    assert_eq!(
        serialize_cells(&[vec![text("a\nb")]], ClipboardFormat::Tsv, &one, None, "", false),
        "\"a\nb\""
    );
    assert_eq!(
        serialize_cells(&[vec![text("he said \"hi\"")]], ClipboardFormat::Tsv, &one, None, "", false),
        "\"he said \"\"hi\"\"\""
    );
    assert_eq!(
        serialize_cells(&[vec![ClipValue::Null]], ClipboardFormat::Tsv, &one, None, "", false),
        ""
    );
    assert_eq!(
        serialize_cells(
            &[vec![ClipValue::Null], vec![text("")]],
            ClipboardFormat::Tsv,
            &one,
            None,
            "NULL",
            false,
        ),
        "NULL\n"
    );
}

#[test]
fn serializer_formats_headers_and_objects() {
    let ids = vec!["id".to_owned()];
    let pair = vec!["x".to_owned(), "y".to_owned()];
    let rows = vec![vec![text("1")]];
    assert_eq!(serialize_cells(&rows, ClipboardFormat::Tsv, &ids, None, "", false), "1");
    assert_eq!(
        serialize_cells(&rows, ClipboardFormat::Tsv, &ids, Some(true), "", false),
        "id\n1"
    );
    assert_eq!(
        serialize_cells(&rows, ClipboardFormat::Csv, &ids, None, "", false),
        "id\n1"
    );
    assert_eq!(
        serialize_cells(&[vec![text("a,b")]], ClipboardFormat::Csv, &vec!["x".into()], None, "", false),
        "x\n\"a,b\""
    );
    assert_eq!(
        serialize_cells(&[vec![text("a"), text("b")]], ClipboardFormat::Json, &pair, None, "", false),
        "[[\"a\",\"b\"]]"
    );
    assert_eq!(
        serialize_cells(&[vec![text("a"), text("b")]], ClipboardFormat::Json, &pair, None, "", true),
        "[{\"x\":\"a\",\"y\":\"b\"}]"
    );
    assert_eq!(
        serialize_cells(&[vec![text("a\tb"), text("c")]], ClipboardFormat::Plain, &pair, None, "", false),
        "a\tb\tc"
    );
    assert_eq!(
        serialize_cells(&[vec![text("<b>&")]], ClipboardFormat::Html, &vec!["x".into()], None, "", false),
        "<table>\n<thead><tr><th>x</th></tr></thead>\n<tbody>\n<tr><td>&lt;b&gt;&amp;</td></tr>\n</tbody>\n</table>"
    );
    assert_eq!(
        serialize_cells(&[vec![text("a|b\nc")]], ClipboardFormat::Markdown, &vec!["x".into()], None, "", false),
        "| x |\n| --- |\n| a\\|b<br>c |"
    );
}

#[test]
fn parser_refuses_empty_ragged_and_flags_unclosed_quote() {
    assert!(matches!(parse_clipboard_text(""), ParseResult::Empty { .. }));
    assert!(matches!(parse_clipboard_text("   "), ParseResult::Empty { .. }));
    assert!(matches!(
        parse_clipboard_text("a,b\nc,d,e"),
        ParseResult::Ragged { row: 1, expected: 2, actual: 3 }
    ));
    assert!(matches!(
        parse_clipboard_text("\"abc"),
        ParseResult::Ok { unclosed_quote: true, .. }
    ));
    assert!(matches!(
        parse_clipboard_text("😀"),
        ParseResult::Ok { byte_length: 4, .. }
    ));
}

#[test]
fn parser_keeps_quoted_delimiters_and_drops_a_trailing_break() {
    let parsed = parse_clipboard_text("\"a\tb\",c\r\n\"x\ny\",\"he said \"\"hi\"\"\"\n");
    let ParseResult::Ok { cells, quoted, multiline, .. } = parsed else {
        panic!("a quoted rectangular block must parse");
    };
    assert_eq!(cells, vec![vec!["a\tb", "c"], vec!["x\ny", "he said \"hi\""]]);
    assert_eq!(quoted, vec![vec![true, false], vec![true, true]]);
    assert!(multiline);
    let single = parse_clipboard_text("a\n");
    assert!(matches!(single, ParseResult::Ok { ref cells, .. } if cells.len() == 1));
    let nulls = parse_clipboard_text("\t");
    assert!(matches!(nulls, ParseResult::Ok { ref cells, ref quoted, .. }
        if cells == &vec![vec!["", ""]] && quoted == &vec![vec![false, false]]));
}

#[test]
fn parser_enforces_byte_and_cell_limits() {
    let huge = "x".repeat(crate::views::table_view_clipboard::GRID_PASTE_MAX_BYTES + 1);
    assert!(matches!(parse_clipboard_text(&huge), ParseResult::TooLarge { .. }));
    let many_cells = "a,".repeat(crate::views::table_view_clipboard::GRID_PASTE_MAX_CELLS + 1);
    assert!(matches!(
        parse_clipboard_text(&many_cells),
        ParseResult::TooLarge { cells, .. } if cells > crate::views::table_view_clipboard::GRID_PASTE_MAX_CELLS
    ));
}

#[test]
fn parse_problem_uses_the_localized_ragged_detail() {
    let problem = clip::parse_problem(&parse_clipboard_text("a,b\nc,d,e")).expect("ragged problem");
    assert_eq!(problem.code, "grid.paste.raggedRow");
    assert_eq!(
        problem.detail.as_deref(),
        Some(
            t("dataTable.pasteRaggedRow")
                .replace("{row}", "2")
                .replace("{count}", "3")
                .replace("{expected}", "2")
                .as_str()
        )
    );
    assert_eq!(
        clip::parse_problem(&parse_clipboard_text("")).map(|p| p.code),
        Some("grid.paste.empty".to_owned())
    );
    assert!(clip::parse_problem(&parse_clipboard_text("a\tb")).is_none());
}

// ── row / value mapping ───────────────────────────────────────────────────

#[test]
fn selection_and_rows_matrices_map_by_name_and_sorted_index() {
    let rows = vec![row(&[("a", text("1")), ("b", text("2"))]), row(&[("a", text("3"))])];
    let coords = vec![
        clip::CellCoord { row_index: 1, column_name: "a".to_owned() },
        clip::CellCoord { row_index: 1, column_name: "b".to_owned() },
    ];
    assert_eq!(
        clip::build_selection_matrix(&rows, &coords),
        vec![vec![text("3"), ClipValue::Null]]
    );
    let order = vec!["a".to_owned(), "b".to_owned()];
    assert_eq!(
        clip::build_rows_matrix(&rows, &order, &[1, 0]),
        vec![vec![text("1"), text("2")], vec![text("3"), ClipValue::Null]]
    );
}

#[test]
fn draft_values_strip_pk_auto_and_unknown_columns() {
    let columns = vec![
        column("id", true, false),
        column("email", false, false),
        column("code", false, true),
        column("note", false, false),
    ];
    let names = vec![
        "id".to_owned(),
        "email".to_owned(),
        "code".to_owned(),
        "note".to_owned(),
        "mystery".to_owned(),
    ];
    let matrix = vec![vec![
        text("1"),
        text("a@b.c"),
        text("X"),
        ClipValue::Null,
        text("ignored"),
    ]];
    let drafts = clip::rows_to_draft_values(&names, &matrix, &columns);
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].len(), 2);
    assert_eq!(drafts[0].get("email"), Some(&text("a@b.c")));
    assert_eq!(drafts[0].get("note"), Some(&ClipValue::Null));
    assert!(drafts[0].get("id").is_none() && drafts[0].get("code").is_none());
}

// ── confirm copy ──────────────────────────────────────────────────────────

#[test]
fn confirm_texts_follow_source_and_size_branches() {
    let p = plan(10);
    let unknown = clip::confirm_texts(&p, "demo");
    assert_eq!(
        unknown.title,
        t("dataTable.pasteCrossTableTitle")
    );
    assert_eq!(
        unknown.message,
        t("dataTable.pasteUnknownSourceBody")
            .replace("{total}", "3")
            .replace("{target}", "demo")
    );
    assert_eq!(unknown.confirm_label, t("dataTable.pasteToSelectedCells"));
    assert_eq!(unknown.cancel_label, t("common.cancel"));

    let mut cross = p.clone();
    cross.cross_table = Some(true);
    cross.dropped_columns = vec!["id".to_owned(), "code".to_owned()];
    let texts = clip::confirm_texts(&cross, "demo");
    assert_eq!(
        texts.message,
        t("dataTable.pasteCrossTableBody")
            .replace("{source}", "other")
            .replace("{target}", "demo")
            .replace("{matched}", "2")
            .replace("{total}", "3")
    );
    assert_eq!(
        texts.description.as_deref(),
        Some(
            t("dataTable.pasteDroppedColumns")
                .replace("{count}", "2")
                .replace("{names}", "id, code")
                .as_str()
        )
    );

    let large = clip::confirm_texts(&plan(2_001), "demo");
    assert_eq!(
        large.title,
        t("dataTable.pasteConfirmTitle").replace("{cells}", "2001")
    );
    assert_eq!(
        large.message,
        t("dataTable.pasteConfirmMessage")
            .replace("{rows}", "3")
            .replace("{cols}", "3")
            .replace("{overwritten}", "3")
    );
}

// ── row copy / paste rows ─────────────────────────────────────────────────

fn row_copy() -> clip::RowCopy {
    clip::RowCopy {
        table_key: "source".to_owned(),
        column_names: vec!["email".to_owned(), "code".to_owned()],
        rows: vec![vec![text("a@b.c"), text("X")]],
    }
}

#[test]
fn row_copy_verification_normalizes_os_newlines_and_rejects_stale_text() {
    let payload = row_copy();
    assert_eq!(st::verify_row_copy(Some((&payload, "x\ty\n")), "x\ty\r\n"), Some(payload.clone()));
    assert_eq!(st::verify_row_copy(Some((&payload, "x\ty")), "something else"), None);
    assert_eq!(st::verify_row_copy(None, "x\ty"), None);
}

#[test]
fn copy_rows_as_drafts_gates_and_maps() {
    let rows = vec![row(&[("email", text("a@b.c"))])];
    let order = vec!["email".to_owned()];
    let cols = vec![column("email", false, false)];
    assert_eq!(st::decide_copy_rows_as_drafts(true, true, &[], &rows, &order, &cols), st::DraftCopyDecision::NoRows);
    assert_eq!(st::decide_copy_rows_as_drafts(false, true, &[0], &rows, &order, &cols), st::DraftCopyDecision::NoCapability);
    assert_eq!(st::decide_copy_rows_as_drafts(true, false, &[0], &rows, &order, &cols), st::DraftCopyDecision::NoCapability);
    let ready = st::decide_copy_rows_as_drafts(true, true, &[0], &rows, &order, &cols);
    assert!(matches!(ready, st::DraftCopyDecision::Ready(ref drafts) if drafts[0].get("email") == Some(&text("a@b.c"))));
    // React's rowsToDraftValues still emits one (empty) record for a
    // primary-key-only selection; the writable-column refusal belongs to the
    // paste-rows path, which checks matched columns before building drafts.
    let pk_only = vec![column("email", true, false)];
    assert!(matches!(
        st::decide_copy_rows_as_drafts(true, true, &[0], &rows, &order, &pk_only),
        st::DraftCopyDecision::Ready(ref drafts) if drafts.len() == 1 && drafts[0].is_empty()
    ));
}

#[test]
fn paste_rows_decision_covers_every_refusal_and_the_cross_table_confirm() {
    let payload = row_copy();
    let recorded = Some((&payload, "x\ty"));
    let cols = vec![column("email", false, false), column("code", false, true)];
    assert_eq!(st::decide_paste_rows(true, false, recorded, Some("x\ty"), &cols, "source", "demo"), st::PasteRowsDecision::NoCapability);
    assert_eq!(st::decide_paste_rows(false, true, recorded, Some("x\ty"), &cols, "source", "demo"), st::PasteRowsDecision::ReadOnly);
    assert_eq!(st::decide_paste_rows(true, true, recorded, None, &cols, "source", "demo"), st::PasteRowsDecision::ReadFailed);
    assert_eq!(st::decide_paste_rows(true, true, recorded, Some("excel text"), &cols, "source", "demo"), st::PasteRowsDecision::NotRowData);
    let pk = vec![column("email", true, false)];
    assert_eq!(st::decide_paste_rows(true, true, recorded, Some("x\ty"), &pk, "source", "demo"), st::PasteRowsDecision::NoWritableColumn);

    let same = st::decide_paste_rows(true, true, recorded, Some("x\ty"), &cols, "source", "demo");
    assert!(matches!(same, st::PasteRowsDecision::Ready(ref drafts) if drafts[0].len() == 1));

    let cross = st::decide_paste_rows(true, true, recorded, Some("x\ty"), &cols, "other", "demo");
    let st::PasteRowsDecision::NeedConfirm(texts) = cross else {
        panic!("a cross-table paste must ask first");
    };
    assert_eq!(texts.title, t("dataTable.pasteRowsCrossTableTitle"));
    assert_eq!(texts.confirm_label, t("dataTable.pasteRows"));
    assert_eq!(texts.description.as_deref(), Some(t("dataTable.pasteDroppedColumns").replace("{count}", "1").replace("{names}", "code").as_str()));
}

// ── host-clock feedback state ─────────────────────────────────────────────

#[test]
fn reported_problems_are_capped_and_cell_errors_keyed() {
    let mut vm = st::ClipboardVm::default();
    for index in 0..101 {
        st::report_problem(
            &mut vm,
            PasteProblem {
                code: format!("grid.paste.{index}"),
                row: Some(1),
                column: Some("email".to_owned()),
                ..PasteProblem::default()
            },
        );
    }
    assert_eq!(vm.problems.len(), clip::MAX_REPORTED_PROBLEMS);
    assert_eq!(vm.cell_errors, vec!["1:email".to_owned()]);
    st::clear_paste_result(&mut vm);
    assert!(vm.problems.is_empty() && vm.cell_errors.is_empty() && vm.last_paste.is_none());
}

#[test]
fn copy_feedback_is_optimistic_request_bound_and_deadline_scoped() {
    let mut vm = st::ClipboardVm::default();
    // `now` is the host clock in milliseconds, matching COPY_FEEDBACK_MS.
    let first = st::begin_copy(&mut vm, 10_000.0);
    assert!(st::copy_visible(&mut vm, 10_000.0));
    assert!(st::copy_visible(&mut vm, 11_199.0));
    let second = st::begin_copy(&mut vm, 11_200.0);
    st::end_copy(&mut vm, first, false); // stale failure must not clear newer success
    assert!(st::copy_visible(&mut vm, 11_200.0));
    st::end_copy(&mut vm, second, true);
    assert!(st::copy_visible(&mut vm, 12_399.0));
    assert!(!st::copy_visible(&mut vm, 12_400.0)); // window edge, not before

    st::native_copied(&mut vm, true, 20_000.0);
    assert!(st::copy_visible(&mut vm, 20_000.0));
    st::native_copied(&mut vm, false, 20_100.0);
    assert!(!st::copy_visible(&mut vm, 20_100.0));
}

#[test]
fn empty_copy_never_lights_up_without_a_flavor() {
    assert!(!st::empty_copy_allowed("", None));
    assert!(!st::empty_copy_allowed("", Some("")));
    assert!(st::empty_copy_allowed("text", None));
    assert!(st::empty_copy_allowed("", Some("<table></table>")));
}

#[test]
fn semicolon_delimiter_splits_rows() {
    let parsed = parse_clipboard_text("a;b\nx;y");
    let ParseResult::Ok { cells, .. } = parsed else {
        panic!("semicolon-delimited block must parse");
    };
    assert_eq!(cells, vec![vec!["a", "b"], vec!["x", "y"]]);
}

#[test]
fn html_without_rows_omits_tbody() {
    let out = serialize_cells(
        &[],
        ClipboardFormat::Html,
        &["x".to_owned()],
        None,
        "",
        false,
    );
    assert_eq!(out, "<table>\n<thead><tr><th>x</th></tr></thead>\n</table>");
}

#[test]
fn markdown_without_names_uses_blank_header() {
    let out = serialize_cells(
        &[vec![text("a"), text("b")]],
        ClipboardFormat::Markdown,
        &[],
        None,
        "",
        false,
    );
    assert_eq!(out, "|  |  |\n| --- | --- |\n| a | b |");
}

#[test]
fn plain_header_is_written_only_with_column_names() {
    let out = serialize_cells(
        &[vec![text("a")]],
        ClipboardFormat::Plain,
        &["x".to_owned()],
        Some(true),
        "NULL",
        false,
    );
    assert_eq!(out, "x\na");
}
