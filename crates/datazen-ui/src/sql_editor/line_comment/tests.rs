use super::*;

fn editor_with_ranges(doc: &str, spans: &[(usize, usize)], main: usize) -> SqlEditor {
    let mut editor = SqlEditor::from_doc(doc);
    editor.selection = Selection {
        ranges: spans.iter().map(|&(anchor, head)| Sel { anchor, head }).collect(),
        main,
    };
    editor
}

fn ranges(editor: &SqlEditor) -> Vec<(usize, usize)> {
    editor.selection().ranges().iter().map(|sel| (sel.anchor, sel.head)).collect()
}

#[test]
fn terminal_selection_at_next_line_start_comments_only_previous_line() {
    let doc = "  select 1;\n    select 2;\nselect 3;";
    let mut editor = editor_with_ranges(doc, &[(2, 12)], 0);
    assert!(editor.toggle_query_line_comments());
    assert_eq!(editor.doc(), "  -- select 1;\n    select 2;\nselect 3;");
    assert_eq!(ranges(&editor), [(5, 15)], "CodeMirror keeps the range inward");
    assert_eq!(editor.undo_stack.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.doc(), doc);
    assert_eq!(ranges(&editor), [(2, 12)]);
    assert!(editor.redo());
    assert_eq!(ranges(&editor), [(5, 15)]);
}

#[test]
fn mixed_comment_and_blank_follow_each_ranges_common_indent() {
    let doc = "    select 1;\n  -- prior\n\n  select 2;";
    let mut editor = editor_with_ranges(doc, &[(0, doc.len())], 0);
    assert!(editor.toggle_query_line_comments());
    assert_eq!(editor.doc(), "  --   select 1;\n  -- -- prior\n\n  -- select 2;");
    assert!(editor.toggle_query_line_comments());
    assert_eq!(editor.doc(), doc);
    assert_eq!(editor.undo_stack.len(), 2);
}

#[test]
fn disjoint_and_same_line_ranges_share_one_history_snapshot() {
    let doc = " \tselect 1;\nother\n    select 2;\n";
    let mut editor = editor_with_ranges(doc, &[(2, 2), (6, 6), (27, 23)], 2);
    assert!(editor.toggle_query_line_comments());
    assert_eq!(editor.doc(), " \t-- select 1;\nother\n    -- select 2;\n");
    assert_eq!(ranges(&editor), [(5, 5), (9, 9), (33, 29)]);
    assert_eq!(editor.selection().main_index(), 2);
    assert_eq!(editor.undo_stack.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.doc(), doc);
    assert_eq!(ranges(&editor), [(2, 2), (6, 6), (27, 23)]);
    assert_eq!(editor.selection().main_index(), 2);
}

#[test]
fn uncomment_maps_colliding_carets_once_and_retains_the_main() {
    let mut editor = editor_with_ranges("-- select 1;\n-- select 2;", &[(0, 0), (2, 2), (16, 24)], 1);
    assert!(editor.toggle_query_line_comments());
    assert_eq!(editor.doc(), "select 1;\nselect 2;");
    assert_eq!(ranges(&editor), [(0, 0), (10, 18)]);
    assert_eq!(editor.selection().main_index(), 0);
}

#[test]
fn blank_single_line_and_unicode_indent_use_real_line_boundaries() {
    let mut blank = editor_with_ranges("  \nselect 2;", &[(1, 1)], 0);
    assert!(blank.toggle_query_line_comments());
    assert_eq!(blank.doc(), "  -- \nselect 2;");
    assert_eq!(ranges(&blank), [(1, 1)]);

    let doc = "　select '中文';\r\n select 2;\rselect 3;";
    let mut unicode = editor_with_ranges(doc, &[(0, doc.len())], 0);
    assert!(unicode.toggle_query_line_comments());
    // The common indentation is zero because the last line is unindented.
    assert_eq!(unicode.doc(), "-- 　select '中文';\r\n--  select 2;\r-- select 3;");
    assert!(unicode.undo());
    assert_eq!(unicode.doc(), doc);
    assert_eq!(ranges(&unicode), [(0, doc.len())]);

    let mut mixed = editor_with_ranges("　select 1;\n select 2;", &[(0, 23)], 0);
    assert!(mixed.toggle_query_line_comments());
    assert_eq!(mixed.doc(), "　-- select 1;\n -- select 2;");
}

#[test]
fn no_changes_while_composing_or_when_all_blank_multiline_selection() {
    let mut editor = editor_with_ranges("  \n\t\n", &[(0, 5)], 0);
    assert!(!editor.toggle_query_line_comments());
    assert!(editor.undo_stack.is_empty());
    editor.set_main_caret(0);
    editor.preedit("临时");
    assert!(!editor.toggle_query_line_comments());
    assert_eq!(editor.composition(), Some("临时"));
    assert_eq!(editor.doc(), "  \n\t\n");
    assert!(editor.undo_stack.is_empty());
}
