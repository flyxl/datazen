//! T220 audit: the formatEditorDocument port, verified against the React
//! source re-read line by line and
//! `format/__tests__/formatEditorDocument.test.ts` (the
//! selection-scope group in full; the `formatSql` options group is
//! T615's formatter half, injected here as a stub whose received
//! options are pinned). The engine's `SqlEditorState::format_document`
//! deepening is exercised through its own component suite.
//!
//! Tester-written: every expectation below is re-derived from the React
//! files.

use datazen_ui::components::sql_editor::format_editor_document::{
    format_editor_document, FormatEdit, FormatRejection, PartialSqlFormatOptions,
    SqlFormatOptionsVm, DEFAULT_SQL_FORMAT_OPTIONS,
};
use datazen_ui::components::sql_editor::format_editor_document::{IndentStyle, KeywordCase};

/// A deterministic stand-in for T615's formatter: squeezes runs of
/// whitespace to single spaces and uppercases the first keyword — a
/// real reflow the "changed nothing" guards can be tested against.
fn reflow(sql: &str) -> String {
    let squeezed = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    match squeezed.char_indices().next() {
        Some((_, first)) if first.is_ascii_alphabetic() => {
            let mut chars = squeezed.chars();
            match chars.next() {
                Some(c) => format!("{}{}", c.to_ascii_uppercase(), chars.as_str()),
                None => squeezed,
            }
        }
        _ => squeezed,
    }
}

fn ok_formatter() -> impl Fn(&str, Option<&str>, &SqlFormatOptionsVm) -> Result<String, FormatRejection> {
    |_sql, _db, _opts| Ok(reflow(_sql))
}

/// React test: formats only the selection and leaves surrounding code
/// byte-identical.
#[test]
fn selection_formats_only_the_selection() {
    let before = "SELECT 1;\n";
    let target = "select  id,name   from users";
    let after = "\nSELECT 3;";
    let doc = format!("{before}{target}{after}");
    let from = before.len();
    let to = from + target.len();

    let edit = format_editor_document(
        &doc,
        from,
        to,
        to,
        Some("postgresql"),
        PartialSqlFormatOptions::default(),
        &ok_formatter(),
    )
    .expect("the selection formats");

    assert_eq!(edit.from, from);
    assert_eq!(edit.to, to);
    assert_eq!(edit.insert, "Select id,name from users");
    // The surrounding halves are untouched by construction (the edit
    // covers exactly the selection).
    let replaced = format!("{}{}{}", &doc[..edit.from], edit.insert, &doc[edit.to..]);
    assert!(replaced.starts_with(before));
    assert!(replaced.ends_with(after));
}

/// React test: keeps the formatted block selected so it can be re-run
/// immediately.
#[test]
fn the_formatted_block_stays_selected() {
    let target = "select  id,name   from users";
    let edit = format_editor_document(
        target,
        0,
        target.len(),
        target.len(),
        Some("postgresql"),
        PartialSqlFormatOptions::default(),
        &ok_formatter(),
    )
    .expect("it formats");
    // selection.empty === false: anchor != head, spanning the block.
    assert_eq!(edit.anchor, 0);
    assert_eq!(edit.head, edit.insert.len());
    assert_ne!(edit.anchor, edit.head);
    // The selected text is the whole (new) document.
    assert_eq!(edit.insert.len(), edit.head - edit.anchor);
}

/// React test: preserves the leading indentation of an indented
/// selection (the block keeps sitting at the user's depth).
#[test]
fn an_indented_selection_keeps_its_indent() {
    let doc = "  select  1";
    let edit = format_editor_document(
        doc,
        2,
        doc.len(),
        doc.len(),
        Some("postgresql"),
        PartialSqlFormatOptions::default(),
        &|sql, _, _| Ok(format!("{sql}\nsecond")),
    )
    .expect("it formats");
    // The first line keeps the (out-of-selection) line prefix; the
    // second line gains the block's own indent.
    assert_eq!(edit.insert, "select  1\n  second");
    let replaced = format!("{}{}{}", &doc[..edit.from], edit.insert, &doc[edit.to..]);
    assert!(replaced.starts_with("  "), "the document keeps its leading indent");
}

/// A mid-line selection owns no indent: nothing is re-indented.
#[test]
fn a_mid_line_selection_owns_no_indent() {
    let doc = "aaaa bbbb";
    let edit = format_editor_document(
        doc,
        5,
        9,
        9,
        None,
        PartialSqlFormatOptions::default(),
        &|sql, _, _| Ok(format!("<{sql}>")),
    )
    .expect("it formats");
    assert_eq!(edit.insert, "<bbbb>");
    assert_eq!(
        edit,
        FormatEdit { from: 5, to: 9, insert: "<bbbb>".to_owned(), anchor: 5, head: 11 }
    );
}

/// React test: formats the whole document when there is no selection
/// (the cursor lands at the statement anchor, an empty selection).
#[test]
fn the_whole_document_formats_without_a_selection() {
    let edit = format_editor_document(
        "select  id,name   from users",
        0,
        0,
        5,
        Some("postgresql"),
        PartialSqlFormatOptions::default(),
        &ok_formatter(),
    )
    .expect("it formats");
    assert_eq!(edit.from, 0);
    assert_eq!(edit.insert, "Select id,name from users");
    // A single statement: the anchor is its content start, and the
    // selection is empty (anchor == head).
    assert_eq!(edit.anchor, 0);
    assert_eq!(edit.head, edit.anchor);
}

/// React test: anchors the cursor to the same statement after a
/// whole-document format — the cursor stays inside the third statement
/// rather than at a stale absolute offset.
#[test]
fn the_cursor_anchors_to_its_statement() {
    let doc = "select 1;\nselect 2;\nselect  3   from t;";
    let cursor = doc.rfind("from").expect("the cursor sits in the third");
    let formatted = "SELECT 1;\nSELECT 2;\nSELECT\n  3\nFROM\n  t;";
    let edit = format_editor_document(
        doc,
        cursor,
        cursor,
        cursor,
        Some("postgresql"),
        PartialSqlFormatOptions::default(),
        &|sql, _, _| {
            assert_eq!(sql, doc);
            Ok(formatted.to_owned())
        },
    )
    .expect("it formats");
    let third = formatted.rfind("SELECT").expect("the third statement");
    assert!(edit.head >= third, "the anchor lands in the third statement");
    assert!(edit.head < formatted.len());
    // And it is the statement's CONTENT start (after the leading
    // newline of its line, not the newline itself).
    assert_eq!(edit.head, third);
}

/// The statement-count-changes fallback: a clamped raw offset when the
/// ordinal no longer exists in the formatted text.
#[test]
fn the_statement_fallback_clamps() {
    let doc = "select 1;\nselect 2;\nselect  3   from t;";
    let cursor = doc.rfind("from").expect("in the third");
    let edit = format_editor_document(
        doc,
        cursor,
        cursor,
        cursor,
        None,
        PartialSqlFormatOptions::default(),
        // The formatter collapses to ONE statement: ordinal 2 is gone.
        &|_, _, _| Ok("SELECT 1;".to_owned()),
    )
    .expect("it formats");
    // min(cursor, formatted.len()) — the cursor was past the short
    // result, so it clamps to the end.
    assert_eq!(edit.head, "SELECT 1;".len());
}

/// React test: a no-op on an empty (whitespace-only) document — the
/// whole-document and the selection branches both guard.
#[test]
fn blank_documents_and_blank_selections_do_nothing() {
    let nothing = format_editor_document(
        "   ",
        0,
        0,
        0,
        None,
        PartialSqlFormatOptions::default(),
        &ok_formatter(),
    );
    assert!(nothing.is_none());
    // A whitespace-only selection is nothing to format either.
    let selection = format_editor_document(
        "a   b",
        1,
        4,
        4,
        None,
        PartialSqlFormatOptions::default(),
        &ok_formatter(),
    );
    assert!(selection.is_none());
}

/// React test: leaves the buffer intact when the formatter cannot parse
/// the input (a rejection reads as None — no edit at all).
#[test]
fn a_rejection_leaves_the_buffer_intact() {
    let doc = "select * from t where x = 'unterminated";
    let edit = format_editor_document(
        doc,
        0,
        0,
        0,
        Some("postgresql"),
        PartialSqlFormatOptions::default(),
        &|_, _, _| Err(FormatRejection),
    );
    assert!(edit.is_none(), "no edit is decided on a rejection");
}

/// React test: false when formatting changes nothing.
#[test]
fn an_unchanged_format_is_a_no_op() {
    let already = "SELECT 1";
    let edit = format_editor_document(
        already,
        0,
        0,
        0,
        Some("postgresql"),
        PartialSqlFormatOptions::default(),
        &|sql, _, _| Ok(sql.to_owned()),
    );
    assert!(edit.is_none());
}

/// The options overlay: `{ ...DEFAULT_SQL_FORMAT_OPTIONS, ...options }`
/// — the defaults, with the caller's fields replacing theirs, and the
/// formatter receives the resolved set plus the dialect verbatim.
#[test]
fn the_options_overlay_resolves_and_travels() {
    assert_eq!(DEFAULT_SQL_FORMAT_OPTIONS.keyword_case, KeywordCase::Upper);
    assert_eq!(DEFAULT_SQL_FORMAT_OPTIONS.indent_style, IndentStyle::TwoSpaces);
    assert!(DEFAULT_SQL_FORMAT_OPTIONS.break_before_boolean_operators);
    assert_eq!(DEFAULT_SQL_FORMAT_OPTIONS.lines_between_queries, 1);

    let partial = PartialSqlFormatOptions {
        keyword_case: Some(KeywordCase::Lower),
        indent_style: Some(IndentStyle::Tab),
        break_before_boolean_operators: Some(false),
        lines_between_queries: Some(2),
    };
    let resolved = SqlFormatOptionsVm::resolve(partial);
    assert_eq!(resolved.keyword_case, KeywordCase::Lower);
    assert_eq!(resolved.indent_style, IndentStyle::Tab);
    assert!(!resolved.break_before_boolean_operators);
    assert_eq!(resolved.lines_between_queries, 2);

    // The empty overlay is the defaults.
    let bare = SqlFormatOptionsVm::resolve(PartialSqlFormatOptions::default());
    assert_eq!(bare, DEFAULT_SQL_FORMAT_OPTIONS);

    // The formatter receives the resolved set and the dialect.
    let edit = format_editor_document(
        "select 1",
        0,
        0,
        0,
        Some("mysql"),
        partial,
        &|sql, database_type, options| {
            assert_eq!(sql, "select 1");
            assert_eq!(database_type, Some("mysql"));
            assert_eq!(options.keyword_case, KeywordCase::Lower);
            assert_eq!(options.lines_between_queries, 2);
            Ok("SELECT 1".to_owned())
        },
    );
    assert!(edit.is_some());
}

fn anchor_for_format(original: &str, formatted: &str, cursor: usize) -> usize {
    let edit = format_editor_document(
        original,
        cursor,
        cursor,
        cursor,
        Some("postgresql"),
        PartialSqlFormatOptions::default(),
        &|sql, _, _| {
            assert_eq!(sql, original);
            Ok(formatted.to_owned())
        },
    )
    .unwrap();
    assert_eq!((edit.from, edit.to), (0, original.len()));
    assert_eq!(edit.insert, formatted);
    assert_eq!(edit.anchor, edit.head);
    assert!(formatted.is_char_boundary(edit.anchor));
    edit.anchor
}

#[test]
fn remapping_ignores_lexical_semicolons_and_counts_delimiter_only_ordinals() {
    let original = "-- old;\nselect [x]];名字], $$💜; else$$, (1;2);;\n    SELECT final;";
    let formatted =
        "-- new;\nSELECT [x]];名字], $$💜; else$$, (1;2);\n    ;\n \tSELECT FINAL;";
    let expected = formatted.find("SELECT FINAL").unwrap();
    // The blank separator is a physical ordinal with no valid content;
    // neither its semicolon nor semicolons inside lexical regions grab caret.
    for offset in [original.find("final").unwrap(), original.rfind('\n').unwrap() + 2] {
        assert_eq!(anchor_for_format(original, formatted, offset), expected);
    }

    let original = "select 'x;''y', \"a;\"\"b\", `c;d`;\n# with query;\nselect 2;";
    let formatted = "    SELECT 'x;''y', \"a;\"\"b\", `c;d`;\n /* new comment */ SELECT 2;";
    assert_eq!(
        anchor_for_format(original, formatted, original.find('#').unwrap()),
        formatted.find("/* new comment */").unwrap(),
    );
}

#[test]
fn remapping_selects_preceding_neighbor_for_leading_whitespace_only() {
    let original = "select 1;\n   -- keep this line;\n   select 2;";
    let formatted = "    SELECT 1;\n\n -- new note\nSELECT 2;";
    assert_eq!(
        anchor_for_format(original, formatted, original.find("   --").unwrap() + 1),
        formatted.find("SELECT 1").unwrap(),
    );
    assert_eq!(
        anchor_for_format(original, formatted, original.find("--").unwrap() + 3),
        formatted.find("-- new note").unwrap(),
    );

    // React locates the source in valid content, but the formatted lookup
    // uses all ranges: ordinal 1 is this semicolon-only span's content start.
    assert_eq!(
        anchor_for_format("select 1; select 2;", "SELECT 1;\n  ;\nSELECT 3;", 13),
        "SELECT 1;\n  ".len(),
    );
}

#[test]
fn pure_comment_fallback_maps_utf16_without_splitting_native_utf8() {
    let original = "--🚲 pure comment\n";
    let formatted = "'🥹🦋';";
    // The caret after the bicycle is 4 UTF-16 units from the start. Target
    // position 4 is between the butterfly's surrogates; floor to its start.
    assert_eq!(
        anchor_for_format(original, formatted, "--🚲".len()),
        formatted.find('🦋').unwrap(),
    );
    assert_eq!(anchor_for_format(original, formatted, original.len()), 0);
    let original = "SELECT 1;\n-- 尾注释;\n";
    assert_eq!(
        anchor_for_format(original, formatted, original.find('尾').unwrap()),
        formatted.len(),
    );
}

#[test]
fn raw_delimiter_and_routine_hints_degrade_the_document_to_one_span() {
    for source in [
        "  DELIMITER $$\nSELECT 1; SELECT 2$$\n  SELECT 3;",
        "\tCREATE\u{a0}OR\u{2003}REPLACE\u{b}FUNCTION f() BEGIN SELECT 2; SELECT 3; END;",
        "SELECT 'delimiter'; SELECT 2; SELECT 3;",
    ] {
        let formatted = "  SELECT A;\n  SELECT B;\nSELECT C;";
        let cursor = source.rfind("SELECT").unwrap();
        assert_eq!(anchor_for_format(source, formatted, cursor), 2);
    }
    let original = "SELECT 0;\nCREATE FUNCTION_ordinary;\nSELECT 3;";
    let formatted = "SELECT A;\nSELECT B;\n SELECT C;";
    assert_eq!(
        anchor_for_format(original, formatted, original.rfind("SELECT").unwrap()),
        formatted.find("SELECT C").unwrap(),
    );
}

#[test]
fn invalid_native_selection_bounds_are_rejected_without_calling_formatter() {
    let doc = "select '🚲';";
    let mid_scalar = doc.find('🚲').unwrap() + 1;
    for (from, to) in [(mid_scalar, doc.len()), (0, mid_scalar), (doc.len(), 0), (0, doc.len() + 1)] {
        assert!(format_editor_document(
            doc, from, to, to, None, PartialSqlFormatOptions::default(),
            &|_, _, _| panic!("an invalid Native selection must not reach the formatter"),
        ).is_none());
    }
}
