use super::*;

fn editor(doc: &str, needle: &str) -> SqlEditor {
    let mut model = SqlEditor::from_doc(doc);
    model.find_open = true;
    model.find_input.text = needle.to_owned();
    model
}

#[test]
fn literal_find_paint_uses_measured_rows_and_ignores_popup_height() {
    let rows = |top, height, sheet, popup| visible_rows((top, height), sheet, 90, 20.0, 12.0, popup);
    assert_eq!(rows(100.0, 64.0, 100.0, None), Some(0..3));
    assert_eq!(rows(100.0, 64.0, 50.0, None), Some(1..6));
    assert_eq!(rows(100.0, 49.0, 50.0, None), Some(1..5));
    assert_eq!(rows(100.0, 64.0, 100.0, Some((20.0, 50.0))), Some(0..1));
    assert_eq!(rows(140.0, 18.0, 100.0, Some((20.0, 50.0))), None);
    assert_eq!(rows(177.0, 30.0, 100.0, Some((20.0, 50.0))), Some(1..3));
}

#[test]
fn literal_find_paint_matches_the_pinned_viewport_cursor_not_global_overlap_phase() {
    let model = editor("aaaaaaa", "aa");
    let paint = FindPaint::for_window(&model, 3..5, false).unwrap();
    assert_eq!(paint.0.iter().map(|hit| hit.range.clone()).collect::<Vec<_>>(), [3..5]);
    let all = FindPaint::for_window(&model, 0..7, false).unwrap();
    assert_eq!(all.0.iter().map(|hit| hit.range.clone()).collect::<Vec<_>>(), [0..2, 2..4, 4..6]);
}

#[test]
fn literal_find_paint_only_marks_exact_selection_and_keeps_unicode_and_lf_bounds() {
    let doc = "FI ﬃ first\r\nfi";
    let mut model = editor(doc, "fi");
    model.find_match_case = true;
    model.find_whole_word = true;
    model.set_main_selection(3, 6);
    let paint = FindPaint::for_window(&model, 0..doc.len(), false).unwrap();
    let hits: Vec<_> = paint.0.iter().map(|hit| (&doc[hit.range.clone()], hit.fill.hex())).collect();
    assert_eq!(hits, [("ﬃ", "#ff6a0054".to_owned()), ("fi", "#ffff0054".to_owned())]);
    let parts: Vec<_> = paint.fragment(3..6).map(|mark| mark.range()).collect();
    assert_eq!(parts, [0..3]);
    assert_eq!(model.selected_text(), "ﬃ");
    assert!(model.undo_stack.is_empty());
    assert_eq!(model.doc(), doc);

    let multiline = editor("foo\r\nbar", "o\\nb");
    let before_break = FindPaint::for_window(&multiline, 0..3, true).unwrap();
    let after_break = FindPaint::for_window(&multiline, 5..8, true).unwrap();
    assert_eq!(before_break.0[0].range, 2..6);
    assert_eq!(after_break.0[0].range, 2..6);
    assert_eq!(before_break.fragment(0..3).next().unwrap().range(), 2..3);
    assert_eq!(after_break.fragment(5..8).next().unwrap().range(), 0..1);
}

#[test]
fn literal_find_paint_skips_unrepresentable_endpoints_without_changing_find_state() {
    let doc = "x𝅗𝅥y 𝅥";
    let model = editor(doc, "𝅥");
    let paint = FindPaint::for_window(&model, 0..doc.len(), true).unwrap();
    assert_eq!(paint.0.iter().map(|hit| &doc[hit.range.clone()]).collect::<Vec<_>>(), ["𝅥"]);
    assert!(model.selected_text().is_empty());
    assert!(!model.find_unsafe_range);
    assert_eq!(model.doc(), doc);

    let mut model = editor("one", "one");
    model.find_open = false;
    assert!(FindPaint::for_window(&model, 0..3, false).is_none());
    let too_many = editor(&"x ".repeat(VISIBLE_MARK_LIMIT + 1), "x");
    assert!(FindPaint::for_window(&too_many, 0..too_many.doc().len(), false).is_none());
}

#[test]
fn regex_context_can_cross_the_window_but_never_paints_hidden_rows() {
    let mut model = editor("ab\ncd\nef", "ab");
    model.set_main_selection(0, 8);
    let paint = FindPaint::from_ranges(&model,[0..8],Some(3..5),false).unwrap();
    assert_eq!(paint.0[0].fill.hex(), "#ff6a0054");
    assert!(paint.fragment(0..2).next().is_none());
    assert_eq!(paint.fragment(3..5).next().unwrap().range(),0..2);
    assert_eq!(paint.fragment(0..8).next().unwrap().range(),3..5);
    assert!(paint.fragment(6..8).next().is_none());
    assert!(model.undo_stack.is_empty());
}
