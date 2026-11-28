use super::*;

#[test]
fn pairs_only_in_confirmed_codemirror_opener_contexts() {
    for next in ["", " ", "\t", "\n", "　", ")", "]", "}", ":", ";", ">"] {
        let mut editor = SqlEditor::from_doc(next);
        assert!(editor.type_query_round_bracket("("), "next {next:?}");
        assert_eq!(editor.doc(), format!("(){next}"));
        assert_eq!(editor.main_head(), 1);
        assert_eq!(editor.undo_stack.len(), 1);
    }
    for next in ["a", ",", "_", "'", "(", "\u{85}"] {
        let mut editor = SqlEditor::from_doc(next);
        assert!(!editor.type_query_round_bracket("("), "next {next:?}");
        assert_eq!(editor.doc(), next);
        assert!(editor.undo_stack.is_empty());
    }
}

#[test]
fn wraps_multirange_utf8_and_keeps_direction_in_one_history_step() {
    let doc = "中文\nlast";
    let mut editor = SqlEditor::from_doc(doc);
    editor.selection = Selection {
        main: 1,
        ranges: vec![Sel { anchor: 0, head: 6 }, Sel { anchor: 11, head: 7 }],
    };
    let original = editor.selection.clone();
    assert!(editor.type_query_round_bracket("("));
    assert_eq!(editor.doc(), "(中文)\n(last)");
    assert_eq!(editor.selection().ranges(),
               [Sel { anchor: 1, head: 7 }, Sel { anchor: 14, head: 10 }]);
    assert_eq!(editor.selection().main_index(), 1);
    assert_eq!(editor.undo_stack.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.doc(), doc);
    assert_eq!(editor.selection(), &original);
    assert!(editor.redo());
    assert_eq!(editor.doc(), "(中文)\n(last)");
}

#[test]
fn mixed_caret_and_range_either_apply_as_one_pair_edit_or_not_at_all() {
    let mut editor = SqlEditor::from_doc("one\ntwo");
    editor.selection = Selection {
        main: 1, ranges: vec![Sel { anchor: 0, head: 3 }, Sel::caret(4)],
    };
    assert!(!editor.type_query_round_bracket("("));
    assert_eq!(editor.doc(), "one\ntwo", "no partly wrapped document");
    assert!(editor.undo_stack.is_empty());

    // The deliberately stricter multiple-caret rule never skips a literal,
    // even alongside a real generated closer at the main caret.
    let mut editor = SqlEditor::from_doc(")");
    assert!(editor.type_query_round_bracket("("));
    editor.add_cursor(2);
    editor.selection.main = 0;
    assert!(!editor.type_query_round_bracket(")"));
    assert_eq!(editor.doc(), "())");
}

#[test]
fn a_closer_follows_inner_edits_and_a_skip_is_not_an_undo_event() {
    let mut editor = SqlEditor::from_doc("");
    assert!(editor.type_query_round_bracket("("));
    editor.insert("中文");
    assert_eq!(editor.doc(), "(中文)");
    assert_eq!(editor.main_head(), 7);
    assert!(editor.type_query_round_bracket(")"));
    assert_eq!(editor.doc(), "(中文)");
    assert_eq!(editor.main_head(), 8);
    assert_eq!(editor.undo_stack.len(), 2);
    assert!(!editor.type_query_round_bracket(")"));
    assert!(editor.undo());
    assert_eq!(editor.doc(), "()");
    assert!(!editor.type_query_round_bracket(")"), "consumed mark cannot resurrect");
}

#[test]
fn history_keeps_only_surviving_marks_and_cannot_certify_a_redone_or_literal_closer() {
    let mut editor = SqlEditor::from_doc("");
    assert!(editor.type_query_round_bracket("("));
    editor.insert("x");
    assert!(editor.undo());
    assert_eq!(editor.doc(), "()");
    assert!(editor.type_query_round_bracket(")"), "surviving mark remaps on undo");

    let mut editor = SqlEditor::from_doc("");
    assert!(editor.type_query_round_bracket("("));
    assert!(editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.doc(), "()");
    assert_eq!(editor.main_head(), 1);
    assert!(!editor.type_query_round_bracket(")"), "redo does not synthesize provenance");

    let mut editor = SqlEditor::from_doc("");
    assert!(editor.type_query_round_bracket("("));
    editor.replace_range(1, 2, ")");
    editor.set_main_caret(1);
    assert!(!editor.type_query_round_bracket(")"), "even identical replacing text is literal");

    let mut editor = SqlEditor::from_doc("");
    editor.insert("()");
    editor.set_main_caret(1);
    assert!(!editor.type_query_round_bracket(")"), "pasting a pair does not mint a mark");
}

#[test]
fn direct_pairing_does_not_run_during_ime_or_retain_marks_across_lines() {
    let mut editor = SqlEditor::from_doc("\nlater");
    assert!(editor.type_query_round_bracket("("));
    editor.set_main_caret(3);
    editor.set_main_caret(1);
    assert!(!editor.type_query_round_bracket(")"));
    editor.preedit("输入法");
    assert!(!editor.type_query_round_bracket("("));
    assert_eq!(editor.doc(), "()\nlater");
    editor.commit("(");
    assert_eq!(editor.doc(), "(()\nlater", "IME commits exactly its text");
}

#[test]
fn backspace_retains_the_base_edit_behavior() {
    let mut editor = SqlEditor::from_doc("");
    assert!(editor.type_query_round_bracket("("));
    editor.backspace();
    assert_eq!(editor.doc(), ")", "the React host did not mount the separate pair-delete keymap");
}
