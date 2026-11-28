//! The live Query editor's scoped, opt-in Find/Replace panel. This is
//! independent of searching loaded result rows and of the plain YAML editor.

use datazen_ui_api::native_query_regex::Operation;
use rune::{col, row, text, El, Key, KeyEvent, Mods, Tx};

use crate::{i18n::t, ui::{field::TextField, toggle::Checkbox}, AppState};

use super::{model::SqlEditor, view::{editor_font, split_lines}};

const FIND_INPUT: &str = "sql-find-input";
const EDITOR_VIEWPORT: &str = "sql-editor-scroll";

fn command(event: &KeyEvent, ch: char) -> bool {
    event.is_command(ch) && !event.mods.alt && (!event.mods.logo || event.mods.cmd)
}

fn direction(event: &KeyEvent) -> Option<bool> {
    let Mods { shift, alt, ctrl, cmd, logo, .. } = event.mods;
    if command(event, 'g') || (event.key == Key::Function(3) && !alt && !ctrl && !cmd && !logo) {
        Some(!shift)
    } else {
        None
    }
}

pub(crate) fn open(state: &mut AppState, tx: &mut Tx) {
    if state.sql.composition().is_some() {
        return;
    }
    // Like the original editor, dock Find in one layout commit rather
    // than letting the outgoing viewport overlap the new controls.
    tx.instant();
    state.result_focused = false;
    state.sql.completion_close();
    if !state.sql.find_input.focused {
        let main = state.sql.selection().main();
        let selected = &state.sql.doc()[main.from()..main.to()];
        if !selected.is_empty() && selected.encode_utf16().count() <= 100 {
            // Keep ordinary single-line input while seeding the short
            // multiline selection as CodeMirror's visible backslash-n.
            let query = selected.replace('\n', "\\n");
            if state.sql.find_input.text != query {
                state.sql.find_input = rune::TextEdit::new(query);
                state.sql.find_unsafe_range = false;
            }
        }
    }
    state.sql.find_open = true;
    state.sql.focused = false;
    state.sql.find_input.anchor = 0;
    state.sql.find_input.focus = state.sql.find_input.text.len();
    tx.focus(FIND_INPUT, tx.by_keys());
}

fn close(state: &mut AppState, tx: &mut Tx) {
    tx.instant();
    state.cancel_query_regex();
    state.sql.dismiss_find();
    state.result_focused = false;
    state.sql.focus();
    tx.focus(EDITOR_VIEWPORT, false);
}

pub(super) fn reveal_selection(editor: &SqlEditor, tx: &mut Tx) {
    let main = editor.selection().main();
    if main.from() == main.to() {
        return;
    }
    let byte = main.from();
    for (line, (start, text)) in split_lines(editor.doc()).iter().copied().enumerate() {
        if byte >= start && byte < start + text.len() {
            // The selection painter gives this exact first fragment a key.
            // Scroll to the actual painted match, not a guessed glyph position
            // or the 2,800pt full-width logical line.
            tx.scroll_into_view(format!("sql-selected-{line}-{}", byte - start));
            break;
        }
    }
}

fn navigate(state: &mut AppState, tx: &mut Tx, next: bool) {
    if state.sql.find_regex {
        if !state.sql.find_open { open(state,tx); return; }
        state.begin_query_regex(if next { Operation::Next } else { Operation::Previous },tx);
        return;
    }
    let found = if next {
        state.sql.find_from_bar()
    } else {
        state.sql.find_previous_from_bar()
    };
    if found {
        reveal_selection(&state.sql, tx);
    }
}

pub(crate) fn canvas_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    if state.sql.composition().is_some() {
        return false;
    }
    if command(event, 'f') && !event.mods.shift {
        open(state, tx);
    } else if let Some(next) = direction(event) {
        if state.sql.find_input.text.is_empty() {
            open(state, tx);
        } else {
            navigate(state, tx, next);
        }
    } else if state.sql.find_open && event.key == Key::Escape && event.mods == Mods::default() {
        close(state, tx);
    } else {
        return false;
    }
    true
}

fn panel_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    if state.sql.find_open {
        // A focused dock button, like a dock field, is not the SQL editor.
        // Leave its plain Enter to Rune's button handling.
        (event.key == Key::Enter && event.mods.cmd) || canvas_key(state, event, tx)
    } else {
        false
    }
}

fn find_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    if event.key == Key::Enter {
        if !event.mods.cmd && !event.mods.ctrl && !event.mods.alt && !event.mods.logo {
            navigate(state, tx, !event.mods.shift);
        }
        // The focused input owns Return, including unsupported chords; only
        // the editor/toolbar may run SQL.
        true
    } else {
        panel_key(state, event, tx)
    }
}

fn replace_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    if event.key == Key::Enter {
        if event.mods == Mods::default() {
            replace_next(state, tx);
        }
        true
    } else {
        panel_key(state, event, tx)
    }
}

fn replace_next(state: &mut AppState, tx: &mut Tx) {
    if state.sql.find_regex {
        state.begin_query_regex(Operation::ReplaceNext,tx);
    } else if state.sql.replace_current_from_bar() {
        reveal_selection(&state.sql, tx);
    }
}

fn button(key: &str, label: &str, ink: rune::Color, dark: bool) -> El {
    let k = crate::ui::kit::kit(dark);
    row().key(key).role("button").label(label).focusable(true).h(22.0).px(6.0)
        .center_y().radius(4.0).fill(k.c.bg_base).border(1.0, k.c.border_base)
        .child(text(label.to_owned()).font(rune::tok::type_::LABEL).color(ink))
}

fn field<'a>(key: &'static str, edit: &'a rune::TextEdit, label: &str, dark: bool) -> TextField<'a> {
    let k = crate::ui::kit::kit(dark);
    TextField::new(key, edit).height(22.0).width(148.0).text_style(editor_font())
        .label(label).placeholder(label).radius(4.0).border(1.0, k.c.border_base)
        .fill(k.c.bg_base)
}

pub(super) fn panel(
    editor: &SqlEditor, ink: rune::Color, dark: bool,
    owner: Option<crate::query_editor_tabs::QueryEditorViewport>, supports_regex: bool,
) -> El {
    const INSET_X: f32 = 6.0;
    const INSET_Y: f32 = 3.0;
    const ROW_H: f32 = 22.0;
    const CLOSE_SIZE: f32 = 20.0;
    let all = owner.clone().map(|owner| {
        let label = t("query.nativeFind.selectAllMatches");
        button("sql-find-select-all-matches", t("query.nativeFind.all"), ink, dark)
            .label(label).help(label)
            .on_press_tx(move |state: &mut AppState, tx| {
                if state.sql.find_open && owner.owns_current_query(state) {
                    if state.sql.find_regex { state.begin_query_regex(Operation::SelectAll, tx); }
                    else { state.sql.select_all_literal_matches_from_bar(); }
                }
            })
    });
    let k = crate::ui::kit::kit(dark);
    let regex = owner.clone().filter(|_| supports_regex).map(|owner| {
        Checkbox::new("sql-find-regex",editor.find_regex).label(t("query.nativeFind.regex"))
            .on_toggle(move |state: &mut AppState| {
                if owner.owns_current_query(state) && state.sql.find_open {
                    state.cancel_query_regex();
                    state.cancel_regex_marks();
                    state.sql.find_regex = !state.sql.find_regex;
                    state.sql.find_unsafe_range = false;
                }
            }).build(k).h(22.0)
    });
    let regex_status=editor.regex_error.and_then(super::regex_find::status).map(|message| {
        text(t(message)).key("sql-find-regex-status").role("status")
            .font(rune::tok::type_::LABEL).color(k.c.danger.fg)
    });
    // Pointer closures are built for one mounted draft. An old tree's action
    // must not start a new request against the newly mounted Query.
    let owns=|owner:Option<crate::query_editor_tabs::QueryEditorViewport>| move |s:&AppState| {
        s.sql.find_open && owner.as_ref().is_none_or(|o|o.owns_current_query(s))
    };
    let next=owns(owner.clone());let prev=owns(owner.clone());
    let current=owns(owner.clone());let replace_all=owns(owner.clone());
    let warning = editor.find_unsafe_range.then(|| {
        text(t("query.nativeFind.unsafeUnicode")).key("sql-find-unsafe-unicode")
            .role("status").font(rune::tok::type_::LABEL).color(k.c.danger.fg)
    });
    // CodeMirror pins Close at the top right, last in keyboard order.
    // Reserve a separate lane so scrollable controls cannot pass under it.
    let close_label = t("resultFind.close");
    let close_button = row()
        .key("sql-find-close")
        .pin(None, Some(INSET_Y + (ROW_H - CLOSE_SIZE) / 2.0), Some(INSET_X), None)
        .size(CLOSE_SIZE, CLOSE_SIZE)
        .center().radius(4.0).hover(k.c.surface_raised)
        .role("button").label(close_label).help(close_label).focusable(true)
        .child(text("×").font(rune::tok::type_::LABEL).color(ink).a11y_hidden())
        .on_press_tx(move |state: &mut AppState, tx| {
            if state.sql.find_open && owner.as_ref().is_none_or(|owner| owner.owns_current_query(state)) {
                close(state, tx);
            }
        });
    col().key("sql-find-bar").w_full().h(52.0).min_w(0.0).gap(2.0).px(INSET_X).py(INSET_Y)
        .role("group").label(t("query.nativeFind.open"))
        .fill(k.c.bg_deep).border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
        .on_key(panel_key)
        .child(row().w_full().min_w(0.0).h(ROW_H).pr(CLOSE_SIZE + INSET_X).child(
            row().key("sql-find-conditions-scroll").grow(1.0).min_w(0.0).h(ROW_H)
                .gap(INSET_X).center_y().scroll_x()
                .child(field(FIND_INPUT, &editor.find_input, t("query.nativeFind.find"), dark)
                    .on_edit(|state: &mut AppState| &mut state.sql.find_input)
                    .on_change(|state: &mut AppState, _| { state.cancel_query_regex(); state.sql.find_unsafe_range = false; })
                    .on_key(find_key).build(dark))
                .child(button("sql-find-next", t("query.nativeFind.next"), ink, dark)
                    .on_press_tx(move |state: &mut AppState, tx| { if next(state) { navigate(state,tx,true); } }))
                .child(button("sql-find-prev", t("query.nativeFind.previous"), ink, dark)
                    .on_press_tx(move |state: &mut AppState, tx| { if prev(state) { navigate(state,tx,false); } }))
                .children(all)
                .child(Checkbox::new("sql-find-match-case", editor.find_match_case)
                    .label(t("query.nativeFind.matchCase"))
                    .on_toggle(|state: &mut AppState| {
                        state.cancel_query_regex();
                        state.sql.find_match_case = !state.sql.find_match_case;
                        state.sql.find_unsafe_range = false;
                    }).build(k).h(22.0))
                .children(regex)
                .child(Checkbox::new("sql-find-whole-word", editor.find_whole_word)
                    .label(t("query.nativeFind.wholeWord"))
                    .on_toggle(|state: &mut AppState| {
                        state.cancel_query_regex();
                        state.sql.find_whole_word = !state.sql.find_whole_word;
                        state.sql.find_unsafe_range = false;
                    }).build(k).h(22.0))
                .children(warning).children(regex_status)
        ))
        .child(
            row().w_full().min_w(0.0).h(22.0).gap(6.0).center_y().scroll_x()
                .child(field("sql-replace-input", &editor.replace_input, t("resultFind.replacementPlaceholder"), dark)
                    .on_edit(|state: &mut AppState| &mut state.sql.replace_input)
                    .on_change(|state: &mut AppState, _| state.cancel_query_regex())
                    .on_key(replace_key).build(dark))
                .child(button("sql-replace-current", t("resultFind.replaceNext"), ink, dark)
                    .on_press_tx(move |state: &mut AppState,tx| { if current(state) { replace_next(state,tx); } }))
                .child(button("sql-replace-all", t("query.nativeFind.replaceAll"), ink, dark)
                    .on_press_tx(move |state: &mut AppState, tx| {
                        if !replace_all(state) {return}
                        if state.sql.find_regex {state.begin_query_regex(Operation::ReplaceAll,tx);}
                        else {state.sql.replace_all_from_bar();}
                    }))
                .child(button("sql-add-cursor", t("query.nativeFind.addCursor"), ink, dark)
                    .on_press(|state: &mut AppState| state.sql.add_cursor_at_end()))
        )
        .child(close_button)
}
