use rune::{Key, KeyEvent, Mods, Tx};

use crate::sql_editor::{SqlEditor, on_key, on_query_key};

fn pressing(editor: &mut SqlEditor, key: Key) -> bool {
    on_query_key(editor, &KeyEvent { key, mods: Mods::default() }, &mut Tx::default())
}

fn at_end(doc: &str) -> SqlEditor {
    let mut editor = SqlEditor::from_doc(doc);
    editor.set_main_caret(doc.len());
    editor.focus();
    editor
}

#[test]
fn function_completion_uses_ported_label_and_apply_as_one_edit() {
    let mut editor = at_end("SELECT ");
    assert!(pressing(&mut editor, Key::Text("cOa".into())));
    assert_eq!(editor.completion_items(), &["COALESCE"] as &[&str]);
    assert_eq!(editor.doc(), "SELECT cOa");
    assert!(pressing(&mut editor, Key::Enter));
    assert_eq!(editor.doc(), "SELECT COALESCE(");
    assert_eq!(editor.main_head(), "SELECT COALESCE(".len());
    assert!(!editor.has_completion());
    assert!(editor.undo());
    assert_eq!(editor.doc(), "SELECT cOa");
    assert_eq!(editor.main_head(), "SELECT cOa".len());
    assert!(editor.redo());
    assert_eq!(editor.doc(), "SELECT COALESCE(");

    let mut stale = at_end("SELECT ");
    assert!(pressing(&mut stale, Key::Text("coa".into())));
    // Simulate a same-length foreign document replacement that still happens
    // to have the same prefix at the old popup's recorded insertion range.
    stale.doc = "DELETE coa".to_owned();
    assert!(pressing(&mut stale, Key::Enter));
    assert_eq!(stale.doc(), "DELETE coa");
    assert_eq!(stale.undo_stack.len(), 1, "no invalid completion history");

    let mut sum = at_end("SELECT DISTINCT ");
    assert!(pressing(&mut sum, Key::Text("sum".into())));
    assert_eq!(sum.completion_items(), &["SUM"] as &[&str]);
    assert!(pressing(&mut sum, Key::Enter));
    assert_eq!(sum.doc(), "SELECT DISTINCT SUM(");
}

#[test]
fn function_completion_ignores_literals_comments_relation_and_split_tokens() {
    for left in [
        "SELECT '", "SELECT \"", "SELECT `", "SELECT [", "SELECT $$",
        "SELECT -- ", "SELECT /* ", "SELECT # ", "SELECT 0; SELECT ",
        "SELECT * FROM ", "SELECT * FROM foo JOIN ", "SELECT foo.",
        "SELECT 1 AS ", "SELECT column_a ", "UPDATE t SET a = ",
    ] {
        let mut editor = at_end(left);
        assert!(pressing(&mut editor, Key::Text("coa".into())));
        assert!(editor.completion_items().is_empty(), "{left:?} offered a function");
    }
    for suffix in ["lesce", " (x, y)"] {
        let seed = format!("SELECT {suffix}");
        let mut editor = at_end(&seed);
        editor.set_main_caret("SELECT ".len());
        assert!(pressing(&mut editor, Key::Text("coa".into())));
        assert!(editor.completion_items().is_empty(), "{suffix:?} offered a rewrite");
        assert_eq!(editor.doc(), format!("SELECT coa{suffix}"));
    }
}

#[test]
fn function_completion_does_not_change_other_canvases_or_multi_range_selection() {
    let mut generic = at_end("SELECT ");
    let input = KeyEvent { key: Key::Text("coa".into()), mods: Mods::default() };
    assert!(on_key(&mut generic, &input, &mut Tx::default()));
    assert!(generic.completion_items().is_empty());

    let mut multiple = at_end("\nSELECT ");
    multiple.set_main_caret(0);
    multiple.add_cursor("\nSELECT ".len());
    assert!(pressing(&mut multiple, Key::Text("coa".into())));
    assert_eq!(multiple.cursor_count(), 2);
    assert!(multiple.completion_items().is_empty());
}
