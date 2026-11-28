//! CodeMirror search 6.6.0's Go to Line grammar, staged without a UI.
//! Columns are zero-based UTF-16 offsets, not display cells. Native floors
//! non-representable targets to the preceding extended grapheme boundary.
use super::navigation::grapheme_edge;

fn take_number(input: &mut &str) -> Option<f64> {
    let count = input.bytes().take_while(u8::is_ascii_digit).count();
    let digits = &input[..count];
    *input = &input[count..];
    if count == 0 { None } else { digits.parse::<f64>().ok() }
}

/// Resolve against the opening snapshot and main selection head. Reject a
/// stale snapshot at the caller. Native renders LF/CRLF, not bare CR.
pub(super) fn target(doc: &str, head: usize, mut input: &str) -> Option<usize> {
    let before = doc.get(..head)?;
    if doc.bytes().enumerate().any(|(at, b)| b == b'\r' && doc.as_bytes().get(at + 1) != Some(&b'\n')) {
        return None;
    }
    let sign = if let Some(rest) = input.strip_prefix('+') {
        input = rest;
        1.0
    } else if let Some(rest) = input.strip_prefix('-') {
        input = rest;
        -1.0
    } else {
        0.0
    };
    let line = take_number(&mut input);
    let column = if let Some(rest) = input.strip_prefix(':') {
        input = rest;
        take_number(&mut input)?
    } else {
        0.0
    };
    let percent = input == "%";
    if !input.is_empty() && !percent {
        return None;
    }

    let current = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let count = doc.bytes().filter(|&b| b == b'\n').count() + 1;
    let line = match line {
        Some(n) if percent => {
            // Keep JavaScript Number's operation order (including rounding
            // toward +infinity at ties) before clamping.
            let mut ratio = n / 100.0;
            if sign != 0.0 {
                ratio = ratio * sign + current as f64 / count as f64;
            }
            let value = count as f64 * ratio;
            let floor = value.floor();
            floor + if value - floor >= 0.5 { 1.0 } else { 0.0 }
        }
        Some(n) if sign != 0.0 => current as f64 + sign * n,
        Some(n) => n,
        None => current as f64,
    }.clamp(1.0, count as f64) as usize;
    let from = if line == 1 { 0 } else {
        doc.match_indices('\n').nth(line - 2)?.0 + 1
    };
    let end = doc[from..].find('\n').map_or(doc.len(), |lf| {
        let at = from + lf;
        if at > from && doc.as_bytes()[at - 1] == b'\r' { at - 1 } else { at }
    });
    let mut used = 0.0;
    let mut byte = from;
    for ch in doc[from..end].chars() {
        used += ch.len_utf16() as f64;
        if used > column { break; }
        byte += ch.len_utf8();
    }
    Some(grapheme_edge(doc, byte, false))
}

#[test]
fn code_mirror_dialog_grammar() {
    let doc = "abc\ndefg\nhij\nkl\nm";
    for (input, expected) in [
        ("2", Some(4)), ("1:1", Some(1)), ("2:0", Some(4)), ("2:99", Some(8)),
        ("+1:2", Some(15)), ("-1:1", Some(5)), ("0:2", Some(2)), ("99", Some(16)),
        ("50%", Some(9)), ("+50%", Some(16)), ("-50%", Some(0)), ("50:2%", Some(11)),
        ("-20:1%", Some(5)), (":2", Some(11)), ("%", Some(9)), ("+", Some(9)),
        ("-", Some(9)), ("+:2%", Some(11)), ("", Some(9)), (" 2", None),
        ("2 ", None), ("2%:1", None), ("2:", None), ("foo", None),
        ("2.5", None), ("٣", None), ("2\u{2028}", None),
    ] {
        assert_eq!(target(doc, 10, input), expected, "{input:?}");
    }
    assert_eq!(target(doc, 10, &"9".repeat(310)), Some(16));
    assert_eq!(target(doc, 10, &format!("{}%", "9".repeat(310))), Some(16));
    assert_eq!(target(doc, 10, &format!("-{}%", "9".repeat(310))), Some(0));
}

#[test]
fn native_graphemes_and_original_line_endings() {
    let doc = "A💖B\r\ne\u{301}x\r\n";
    assert_eq!(target(doc, 0, "1:1"), Some(1));
    assert_eq!(target(doc, 0, "1:2"), Some(1)); // surrogate midpoint
    assert_eq!(target(doc, 0, "1:3"), Some(5));
    assert_eq!(target(doc, 0, "1:4"), Some(6));
    assert_eq!(target(doc, 0, "2:0"), Some(8));
    assert_eq!(target(doc, 0, "2:1"), Some(8)); // before combining mark
    assert_eq!(target(doc, 0, "2:2"), Some(11));
    assert_eq!(target(doc, 0, "3:99"), Some(14));
    assert_eq!(target(doc, 0, "4:0"), Some(14));
    assert_eq!(target(doc, 2, "1"), None);
    assert_eq!(target("a\rb\rc", 0, "2"), None); // explicit native limitation
}

use std::rc::Rc;
use rune::{Chord, ChordKey, ChordMod, El, Key, KeyEvent, Mods, Tx, row, text};
use crate::{AppState, i18n::t, query_editor_tabs::QueryEditorViewport, ui::field::TextField};
use super::{SqlEditor, editor_font};

const EDITOR: &str = "sql-editor-scroll";
const INPUT: &str = "sql-goto-line-input";
const GO: &str = "sql-goto-line-go";
const CLOSE: &str = "sql-goto-line-close";

#[derive(Debug)]
pub(crate) struct Lease {
    draft: QueryEditorViewport,
    revision: u64,
    head: usize,
    identity: Rc<()>,
}

fn available(state: &AppState) -> bool {
    state.snippet_edit_owner().is_some()
        && state.query_snippet_menu_owner().is_none()
        && !state.result_focused
        && !state.sql.find_open
}

pub(super) fn open_key(state: &mut AppState, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let key = Chord::new(ChordKey::Char('g')).with(ChordMod::Primary).with(ChordMod::Alt);
    if !key.matches_tx(ev, tx) || !state.owns_query_document_shortcut(tx) || !available(state) {
        return false;
    }
    if !tx.key_repeat() {
        let Some(draft) = state.query_editor_viewport() else { return false };
        let head = state.sql.selection().main().head;
        let Some(before) = state.sql.doc().get(..head) else { return true };
        let value = (1 + before.bytes().filter(|&b| b == b'\n').count()).to_string();
        let len = value.len();
        state.sql.goto_input = rune::TextEdit::new(value);
        state.sql.goto_input.anchor = 0;
        state.sql.goto_input.focus = len;
        state.sql.goto_lease = Some(Lease {
            draft, revision: state.sql.document_revision, head, identity: Rc::new(()),
        });
        state.sql.completion_close();
        state.sql.focused = false;
        tx.instant();
        tx.focus(INPUT, tx.by_keys());
    }
    true
}

fn owned(lease: &Lease, state: &AppState) -> bool {
    available(state) && lease.draft.owns_current_query(state)
}

pub(crate) fn holds_focus(state: &AppState, tx: &Tx) -> bool {
    state.sql.goto_lease.is_some() && matches!(tx.focused(), Some(INPUT | GO | CLOSE))
}

pub(crate) fn retire_invalid(state: &mut AppState) -> bool {
    if state.sql.goto_lease.as_ref().is_some_and(|lease| !owned(lease, state)) {
        state.sql.goto_lease = None;
        return true;
    }
    false
}

fn finish(state: &mut AppState, tx: &mut Tx, identity: &Rc<()>, submit: bool) {
    let Some(lease) = state.sql.goto_lease.as_ref() else { return };
    if !Rc::ptr_eq(identity, &lease.identity) { return; }
    let owner = owned(lease, state);
    let at = (owner && submit && lease.revision == state.sql.document_revision)
        .then(|| target(state.sql.doc(), lease.head, &state.sql.goto_input.text))
        .flatten();
    state.sql.goto_lease = None;
    if !owner { return; }
    state.sql.focus();
    tx.instant();
    tx.focus(EDITOR, false);
    if let Some(at) = at {
        state.sql.set_main_caret(at);
        tx.scroll_center_in_instant(EDITOR, "sql-caret");
    }
}

fn button(key: &str, label: &str, ink: rune::Color) -> El {
    row().key(key).role("button").label(label).focusable(true).h(24.0).px(6.0)
        .center_y().child(text(label.to_owned()).font(editor_font()).color(ink))
}

pub(super) fn panel(editor: &SqlEditor, ink: rune::Color, dark: bool) -> El {
    let Some(lease) = editor.goto_lease.as_ref() else { return row() };
    let input = lease.identity.clone();
    let keyboard = lease.identity.clone();
    let go = lease.identity.clone();
    let close = lease.identity.clone();
    let k = crate::ui::kit::kit(dark);
    let label = t("query.nativeGotoLine.label");
    let go_label = t("query.nativeGotoLine.submit");
    let close_label = t("resultFind.close");
    row().key("sql-goto-line-panel").w_full().h(34.0).px(6.0).pr(36.0).gap(8.0).center_y()
        .role("group").label(label).fill(k.c.bg_deep)
        .on_key(move |state: &mut AppState, ev, tx| {
            if ev.key == Key::Escape && ev.mods == Mods::default() {
                finish(state, tx, &keyboard, false);
                true
            } else {
                ev.key == Key::Enter && ev.mods != Mods::default()
            }
        })
        .child(text(t("query.nativeGotoLine.prompt")).font(editor_font()).color(ink))
        .child(TextField::new(INPUT, &editor.goto_input).height(24.0).width(120.0)
            .label(label).text_style(editor_font())
            .on_edit(|state: &mut AppState| &mut state.sql.goto_input)
            .on_key(move |state: &mut AppState, ev, tx| {
                if ev.key == Key::Enter && ev.mods == Mods::default() {
                    finish(state, tx, &input, true);
                    true
                } else if ev.key == Key::Escape && ev.mods == Mods::default() {
                    finish(state, tx, &input, false);
                    true
                } else {
                    ev.key == Key::Enter
                }
            }).build(dark))
        .child(button(GO, go_label, ink).help(go_label).on_press_tx(move |state: &mut AppState, tx| finish(state, tx, &go, true)))
        .child(row().key(CLOSE).pin(None, Some(5.0), Some(6.0), None)
            .size(24.0, 24.0).center().radius(4.0).hover(k.c.surface_raised)
            .role("button").label(close_label).help(close_label).focusable(true)
            .child(text("×").font(editor_font()).color(ink).a11y_hidden())
            .on_press_tx(move |state: &mut AppState, tx| finish(state, tx, &close, false)))
}
