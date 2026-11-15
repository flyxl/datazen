use super::*;
use rune::{Ime, Key, KeyEvent, Span, TextEdit, Tx};


/// T1: `on_key` answers through a `Tx`; the unit tests press with a bare one.
fn press(editor: &mut SqlEditor, ev: KeyEvent) -> bool {
    on_key(editor, &ev, &mut Tx::default())
}

#[test]
fn keyword_table_is_sorted() {
    let mut sorted = crate::sql_editor::model::KEYWORDS.to_vec();
    sorted.sort_unstable();
    assert_eq!(crate::sql_editor::model::KEYWORDS, sorted.as_slice());
}

#[test]
fn highlights_keywords_strings_comments_and_numbers() {
    let src = "SELECT 42 FROM t WHERE name = '雪' -- tail\n";
    let spans = highlight_sql(src, true);
    let slice = |span: &Span| &src[span.start as usize..span.end as usize];
    assert!(spans.iter().any(|s| slice(s) == "SELECT"));
    assert!(spans.iter().any(|s| slice(s) == "FROM"));
    assert!(spans.iter().any(|s| slice(s) == "WHERE"));
    assert!(spans.iter().any(|s| slice(s) == "42"));
    assert!(spans.iter().any(|s| slice(s) == "'雪'"));
    assert!(spans.iter().any(|s| slice(s) == "-- tail"));
}

#[test]
fn block_comment_does_not_color_the_keyword_inside_it() {
    let src = "/* SELECT */ FROM";
    let spans = highlight_sql(src, true);
    let slice = |span: &Span| &src[span.start as usize..span.end as usize];
    assert!(spans.iter().any(|s| slice(s) == "/* SELECT */"));
    assert!(spans.iter().any(|s| slice(s) == "FROM"));
    assert!(!spans.iter().any(|s| slice(s) == "SELECT"));
}

#[test]
fn change_by_range_inserts_at_every_caret() {
    let mut editor = SqlEditor::from_doc("ab");
    editor.add_cursor(2);
    editor.insert("X");
    assert_eq!(editor.doc(), "XabX");
    assert_eq!(editor.cursor_count(), 2);
    assert_eq!(editor.main_head(), 4, "the added caret is main, CodeMirror addCursor");
    assert_eq!(editor.extra_carets(), vec![1]);
}

#[test]
fn composition_stays_out_of_the_document_until_commit_and_maps_other_carets() {
    let mut editor = SqlEditor::from_doc("ab");
    editor.focus();
    editor.add_cursor(2);
    editor.selection.main = 0;
    editor.preedit("中");
    assert_eq!(editor.doc(), "ab");
    assert_eq!(editor.composition(), Some("中"));
    editor.commit("中文");
    assert_eq!(editor.composition(), None);
    assert_eq!(editor.doc(), "中文ab");
    assert_eq!(editor.main_head(), "中文".len());
    assert_eq!(editor.extra_carets(), vec!["中文".len() + 2]);
}

#[test]
fn typing_then_undo_returns_the_document() {
    let mut editor = SqlEditor::from_doc("ab");
    editor.focus();
    editor.insert("X");
    editor.insert("Y");
    assert_eq!(editor.doc(), "XYab", "the caret sits before the text");
    assert!(editor.undo());
    assert_eq!(editor.doc(), "Xab", "one undo step per insert");
    assert!(editor.undo());
    assert_eq!(editor.doc(), "ab", "back to the original document");
    assert!(!editor.undo(), "the history is empty");
}

#[test]
fn delete_is_undoable_and_redo_restores_it() {
    let mut editor = SqlEditor::from_doc("abc");
    editor.focus();
    editor.selection = Selection::single(3);
    editor.backspace();
    assert_eq!(editor.doc(), "ab");
    assert!(editor.undo());
    assert_eq!(editor.doc(), "abc", "undo restores the deleted character");
    assert!(!editor.undo(), "one change, one undo step");
    assert!(editor.redo());
    assert_eq!(editor.doc(), "ab");
    assert!(!editor.redo(), "the redo stack is empty again");
}

#[test]
fn a_fresh_edit_clears_the_redo_stack() {
    let mut editor = SqlEditor::from_doc("");
    editor.focus();
    editor.insert("a");
    editor.undo();
    assert_eq!(editor.doc(), "");
    assert!(editor.redo());
    assert_eq!(editor.doc(), "a");
    editor.insert("b");
    assert!(!editor.redo(), "the redo stack was cleared by the new insert");
    assert_eq!(editor.doc(), "ab");
}

#[test]
fn undo_restores_the_selection_with_the_document() {
    let mut editor = SqlEditor::from_doc("ab");
    editor.focus();
    editor.insert("X");
    editor.undo();
    assert_eq!(editor.main_head(), 0, "the caret returns to where the insert happened");
}

#[test]
fn command_z_and_command_shift_z_go_through_on_key() {
    let mut editor = SqlEditor::from_doc("ab");
    editor.focus();
    assert!(press(&mut editor, KeyEvent { key: Key::Text("c".into()), mods: rune::Mods::default() }));
    assert_eq!(editor.doc(), "cab");
    assert!(press(&mut editor, KeyEvent { key: Key::Command('z'), mods: rune::Mods::default() }));
    assert_eq!(editor.doc(), "ab");
    assert!(press(
        &mut editor,
        KeyEvent { key: Key::Command('z'), mods: rune::Mods { shift: true, ..rune::Mods::default() } }
    ));
    assert_eq!(editor.doc(), "cab", "shift-command-z redoes");
    assert!(press(&mut editor, KeyEvent { key: Key::Command('y'), mods: rune::Mods::default() }) == false
        || editor.doc() == "cab");
}

#[test]
fn find_next_selects_occurrences_in_order_and_wraps() {
    let mut editor = SqlEditor::from_doc("a X b X c X");
    editor.focus();
    assert!(editor.find_next("X"));
    assert_eq!(editor.selection.main().from(), 2);
    assert!(editor.find_next("X"));
    assert_eq!(editor.selection.main().from(), 6);
    assert!(editor.find_next("X"));
    assert_eq!(editor.selection.main().from(), 10);
    assert!(editor.find_next("X"), "the search wraps around");
    assert_eq!(editor.selection.main().from(), 2);
    assert!(!editor.find_next("Y"), "a missing needle selects nothing");
}

#[test]
fn replace_current_replaces_the_selection_and_moves_to_the_next_match() {
    let mut editor = SqlEditor::from_doc("cat dog cat");
    editor.focus();
    editor.find_next("cat");
    assert!(editor.replace_current("cat", "bird"));
    assert_eq!(editor.doc(), "bird dog cat");
    assert_eq!(editor.selection.main().from(), 9, "the next match is selected");
    assert!(editor.undo(), "replacements are undoable");
    assert_eq!(editor.doc(), "cat dog cat");
}

#[test]
fn replace_all_swaps_every_occurrence() {
    let mut editor = SqlEditor::from_doc("one two one two one");
    editor.focus();
    assert_eq!(editor.replace_all("one", "1"), 3);
    assert_eq!(editor.doc(), "1 two 1 two 1");
    assert!(editor.undo());
    assert_eq!(editor.doc(), "one two one two one");
}

#[test]
fn replace_all_leaves_unmatched_documents_alone() {
    let mut editor = SqlEditor::from_doc("abc");
    editor.focus();
    assert_eq!(editor.replace_all("zzz", "y"), 0);
    assert_eq!(editor.doc(), "abc");
    assert!(!editor.undo(), "no change, no history entry");
}

#[test]
fn the_find_bar_reads_its_own_inputs() {
    let mut editor = SqlEditor::from_doc("old and old");
    editor.focus();
    editor.find_input = TextEdit::new("old");
    editor.replace_input = TextEdit::new("new");
    assert!(editor.find_from_bar());
    assert_eq!(editor.selection.main().from(), 0);
    assert_eq!(editor.replace_all_from_bar(), 2);
    assert_eq!(editor.doc(), "new and new");
}

#[test]
fn statements_split_at_semicolons_and_skip_empty_ones() {
    let editor = SqlEditor::from_doc("a;; b ;  c");
    let texts: Vec<&str> = editor
        .statements()
        .iter()
        .map(|(start, end)| &editor.doc()[*start..*end])
        .collect();
    assert_eq!(texts, vec!["a", " b ", "  c"]);
}

#[test]
fn the_cursor_picks_its_own_statement() {
    let mut editor = SqlEditor::from_doc("first; second; third");
    editor.focus();
    editor.selection = Selection::single(10);
    assert_eq!(editor.statement_at_cursor().as_deref(), Some("second"), "inside the second");
    editor.selection = Selection::single(0);
    assert_eq!(editor.statement_at_cursor().as_deref(), Some("first"));
    editor.selection = Selection::single(7);
    assert_eq!(
        editor.statement_at_cursor().as_deref(),
        Some("second"),
        "right after a semicolon picks the next statement"
    );
    editor.selection = Selection::single(editor.doc().len());
    assert_eq!(editor.statement_at_cursor().as_deref(), Some("third"));
}

#[test]
fn a_document_without_semicolons_is_one_statement() {
    let mut editor = SqlEditor::from_doc("select 1");
    editor.focus();
    assert_eq!(editor.statement_at_cursor().as_deref(), Some("select 1"));
}

#[test]
fn an_empty_document_has_no_statement() {
    let editor = SqlEditor::from_doc("");
    assert_eq!(editor.statement_at_cursor(), None);
    let editor = SqlEditor::from_doc("  ;  ");
    assert_eq!(editor.statement_at_cursor(), None, "whitespace-only statements do not count");
}

#[test]
fn typing_sel_offers_select_and_enter_inserts_it() {
    let mut editor = SqlEditor::from_doc("");
    editor.focus();
    for ch in ["S", "E", "L"] {
        assert!(press(&mut editor, KeyEvent { key: Key::Text(ch.into()), mods: rune::Mods::default() }));
    }
    assert_eq!(editor.completion_items(), &["SELECT"] as &[&str]);
    assert!(press(&mut editor, KeyEvent { key: Key::Enter, mods: rune::Mods::default() }));
    assert_eq!(editor.doc(), "SELECT", "Enter inserts the keyword over the prefix");
    assert!(editor.completion_items().is_empty(), "the list closes on insert");
    assert_eq!(editor.main_head(), "SELECT".len());
    assert!(editor.undo(), "the completion insert is undoable");
    assert_eq!(editor.doc(), "SEL");
}

#[test]
fn arrow_keys_cycle_the_completion_and_escape_closes() {
    let mut editor = SqlEditor::from_doc("");
    editor.focus();
    press(&mut editor, KeyEvent { key: Key::Text("S".into()), mods: rune::Mods::default() });
    assert_eq!(editor.completion_items(), &["SELECT", "SET", "SUM"] as &[&str]);
    press(&mut editor, KeyEvent { key: Key::Down, mods: rune::Mods::default() });
    assert_eq!(editor.completion_selected(), 1);
    press(&mut editor, KeyEvent { key: Key::Down, mods: rune::Mods::default() });
    press(&mut editor, KeyEvent { key: Key::Down, mods: rune::Mods::default() });
    assert_eq!(editor.completion_selected(), 0, "down wraps around");
    press(&mut editor, KeyEvent { key: Key::Up, mods: rune::Mods::default() });
    assert_eq!(editor.completion_selected(), 2, "up wraps around");
    press(&mut editor, KeyEvent { key: Key::Escape, mods: rune::Mods::default() });
    assert!(editor.completion_items().is_empty());
    assert_eq!(editor.doc(), "S", "escape only closes the list");
}

#[test]
fn typing_narrows_the_completion_and_no_match_closes_it() {
    let mut editor = SqlEditor::from_doc("");
    editor.focus();
    for ch in ["S", "E"] {
        press(&mut editor, KeyEvent { key: Key::Text(ch.into()), mods: rune::Mods::default() });
    }
    assert_eq!(editor.completion_items(), &["SELECT", "SET"] as &[&str]);
    press(&mut editor, KeyEvent { key: Key::Backspace, mods: rune::Mods::default() });
    assert_eq!(editor.completion_items().len(), 3, "backspace reopens the wider list");
    press(&mut editor, KeyEvent { key: Key::Text("Q".into()), mods: rune::Mods::default() });
    assert!(editor.completion_items().is_empty(), "no keyword starts with SQ");
    press(&mut editor, KeyEvent { key: Key::Enter, mods: rune::Mods::default() });
    assert_eq!(editor.doc(), "SQ\n", "Enter is a newline once the list is closed");
}

#[test]
fn completion_matches_ignore_case() {
    let mut editor = SqlEditor::from_doc("");
    editor.focus();
    for ch in ["s", "e", "l"] {
        press(&mut editor, KeyEvent { key: Key::Text(ch.into()), mods: rune::Mods::default() });
    }
    assert_eq!(editor.completion_items(), &["SELECT"] as &[&str]);
    press(&mut editor, KeyEvent { key: Key::Enter, mods: rune::Mods::default() });
    assert_eq!(editor.doc(), "SELECT");
}

#[test]
fn typing_is_ignored_while_a_composition_is_open() {
    let mut editor = SqlEditor::from_doc("ab");
    editor.focus();
    editor.preedit("中");
    assert!(!press(&mut editor, KeyEvent { key: Key::Text("A".into()), mods: rune::Mods::default() }));
    assert_eq!(editor.doc(), "ab");
}
