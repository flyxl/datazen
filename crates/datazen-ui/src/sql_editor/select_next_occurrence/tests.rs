use super::*;

fn select(editor: &mut SqlEditor, ranges: &[(usize, usize)], main: usize) {
    editor.selection = Selection {
        ranges: ranges.iter().map(|&(anchor, head)| Sel { anchor, head }).collect(),
        main,
    };
}

fn assert_shape(editor: &SqlEditor, ranges: &[(usize, usize)], main: usize) {
    assert_eq!(editor.selection.main_index(), main);
    assert_eq!(editor.selection.ranges().iter().map(|sel| (sel.anchor, sel.head)).collect::<Vec<_>>(), ranges);
}

#[test]
fn next_occurrence_expands_then_wraps_from_the_rightmost_and_keeps_main() {
    let raw = "cat CAT cat xcat cat";
    let mut editor = SqlEditor::from_doc(raw);
    editor.set_main_caret(19);
    let revision = editor.document_revision;
    assert_eq!(editor.select_next_query_occurrence(), Some(SelectionAdvance::ExpandedWords));
    assert_shape(&editor, &[(17,20)], 0);
    assert_eq!(editor.select_next_query_occurrence(), Some(SelectionAdvance::Added(secondary_caret_key(3))));
    assert_shape(&editor, &[(0,3), (17,20)], 1);
    assert_eq!(editor.select_next_query_occurrence(), Some(SelectionAdvance::Added(secondary_caret_key(11))));
    assert_shape(&editor, &[(0,3), (8,11), (17,20)], 2);
    assert_eq!(editor.select_next_query_occurrence(), None);
    assert_eq!(editor.doc(), raw);
    assert_eq!(editor.document_revision, revision);
    assert!(editor.undo_stack.is_empty() && editor.redo_stack.is_empty());

    editor.insert("z");
    assert_eq!(editor.doc(), "z CAT z xcat z");
    assert!(editor.undo());
    assert_eq!(editor.doc(), raw);
    assert_shape(&editor, &[(0,3), (8,11), (17,20)], 2);
    assert!(!editor.undo());
}

#[test]
fn next_occurrence_preserves_direction_and_normalizes_overlap_without_moving_main() {
    let mut editor = SqlEditor::from_doc("cat CAT cat xcat cat");
    editor.set_main_selection(11, 8);
    assert_eq!(editor.select_next_query_occurrence(), Some(SelectionAdvance::Added(secondary_caret_key(20))));
    assert_shape(&editor, &[(11,8), (17,20)], 0);
    assert_eq!(editor.select_next_query_occurrence(), Some(SelectionAdvance::Added(secondary_caret_key(3))));
    assert_shape(&editor, &[(0,3), (11,8), (17,20)], 1);

    let mut overlap = SqlEditor::from_doc("banana....ana");
    select(&mut overlap, &[(6,3),(10,13)], 0);
    assert_eq!(overlap.select_next_query_occurrence(), Some(SelectionAdvance::Added("sql-line-0".to_owned())));
    assert_shape(&overlap, &[(6,1),(10,13)], 0);
}

#[test]
fn next_occurrence_matches_word_at_for_mixed_carets_and_partial_literals() {
    let mut mixed = SqlEditor::from_doc("cat dog cat");
    // Native input historically stores an appended caret, even when it
    // occurs to the left of another range. CM always uses sorted ranges.
    select(&mut mixed, &[(5,5), (3,2)], 0);
    assert_eq!(mixed.select_next_query_occurrence(), Some(SelectionAdvance::ExpandedWords));
    assert_shape(&mixed, &[(0,3), (4,7)], 1);
    assert_eq!(mixed.select_next_query_occurrence(), None);

    let mut partial = SqlEditor::from_doc("cat xcat CAT");
    partial.set_main_selection(3, 1);
    assert!(partial.select_next_query_occurrence().is_some());
    assert_shape(&partial, &[(3,1), (6,8)], 0);

    let mut words = SqlEditor::from_doc("🪴 中β9  \n\u{301}\t$foo");
    let interior = words.doc().find('β').unwrap();
    words.set_main_caret(interior);
    assert_eq!(words.select_next_query_occurrence(), Some(SelectionAdvance::ExpandedWords));
    assert_eq!(words.selected_text(), "中β9");
    words.set_main_caret(words.doc().find("foo").unwrap());
    assert_eq!(words.select_next_query_occurrence(), Some(SelectionAdvance::ExpandedWords));
    assert_eq!(words.selected_text(), "foo", "pinned SQL modes don't make $ a word char");
}

#[test]
fn next_occurrence_clips_before_the_rightmost_and_has_no_panel_cap() {
    let mut adjacent = SqlEditor::from_doc("xxx");
    adjacent.set_main_selection(2, 3);
    assert!(adjacent.select_next_query_occurrence().is_some());
    assert_shape(&adjacent, &[(0,1), (2,3)], 1);
    assert_eq!(adjacent.select_next_query_occurrence(), None);

    let mut many = SqlEditor::from_doc("x".repeat(1105));
    let ranges = (0..1002).map(|i| (i, i + 1)).collect::<Vec<_>>();
    select(&mut many, &ranges, 0);
    assert!(many.select_next_query_occurrence().is_some());
    assert_eq!(many.selection.len(), 1003);
    assert_eq!(many.selection.main_index(), 0);
    assert_eq!(many.selection.ranges().last().map(Sel::from), Some(1002));
}

#[test]
fn next_occurrence_uses_projected_exact_equality_and_safe_utf8() {
    let mut unicode = SqlEditor::from_doc("É E\u{301} É");
    unicode.set_main_selection(0, "É".len());
    assert!(unicode.select_next_query_occurrence().is_some());
    assert_shape(&unicode, &[(0,2), (3,6)], 0);
    assert_eq!(unicode.select_next_query_occurrence(), None, "selected texts differ before NFKD");
    let mut newline = SqlEditor::from_doc("x\r\nz x\nz x\rz");
    select(&mut newline, &[(0,4), (5,8)], 0);
    assert!(newline.select_next_query_occurrence().is_some());
    assert_shape(&newline, &[(0,4), (5,8), (9,12)], 0);
    assert_eq!(newline.doc(), "x\r\nz x\nz x\rz");

    let mut unsafe_hit = SqlEditor::from_doc("x𝅗𝅥y 𝅥");
    let start = unsafe_hit.doc().rfind('𝅥').unwrap();
    unsafe_hit.set_main_selection(start, unsafe_hit.doc().len());
    let selection = unsafe_hit.selection.clone();
    assert_eq!(unsafe_hit.select_next_query_occurrence(), None, "CM can split a surrogate; never truncate Native text");
    assert_eq!(unsafe_hit.selection, selection);

    for (text, at) in [("x\r\nz", 2), ("猫", 1)] {
        let mut invalid = SqlEditor::from_doc(text);
        select(&mut invalid, &[(at, at)], 0);
        assert_eq!(invalid.select_next_query_occurrence(), None);
        assert_shape(&invalid, &[(at,at)], 0);
    }
}

#[test]
fn next_occurrence_no_word_or_ime_cannot_change_selection_document_or_history() {
    let mut editor = SqlEditor::from_doc("  ,  ");
    editor.set_main_caret(2);
    assert_eq!(editor.select_next_query_occurrence(), None);
    assert_shape(&editor, &[(2,2)], 0);
    assert!(editor.undo_stack.is_empty());

    let mut marked = SqlEditor::from_doc("cat cat");
    marked.set_main_caret(2);
    marked.composition = Some("候".to_owned());
    assert_eq!(marked.select_next_query_occurrence(), None);
    assert_shape(&marked, &[(2,2)], 0);
    assert_eq!(marked.composition(), Some("候"));
    assert_eq!(marked.doc(), "cat cat");
    assert!(marked.undo_stack.is_empty());
}
