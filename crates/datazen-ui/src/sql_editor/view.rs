use rune::{El, Ime, Key, KeyEvent, Span, TextEdit, TextStyle, Tx, col, oklch, row, text, text_input};

use super::highlight::highlight_sql;
use super::model::SqlEditor;

/// Editing surface width. The scroll viewport is narrower, so a long line moves sideways
/// instead of wrapping. CodeMirror's `lineWrapping` off does this inside the line layout;
/// Rune's multi-line field cannot turn wrapping off, so the view is not a text field.
pub const EDITOR_SURFACE_WIDTH: f32 = 2800.0;

pub fn editor_font() -> TextStyle {
    TextStyle::new(13.0, 400.0)
}

/// The editor page: a scroll view of unwrapped lines. Not a `text_input`.
pub fn panel(editor: &SqlEditor, ink: rune::Color, muted: rune::Color, dark: bool) -> El {
    let spans = highlight_sql(editor.doc(), dark);
    let carets: Vec<(usize, bool)> = editor
        .selection
        .ranges()
        .iter()
        .enumerate()
        .map(|(i, sel)| (sel.head, i == editor.selection().main_index()))
        .collect();
    let main_head = editor.main_head();
    let mut doc = col().key("sql-editor-field").w(EDITOR_SURFACE_WIDTH).min_h(300.0);
    for (n, (start, line)) in split_lines(editor.doc()).into_iter().enumerate() {
        doc = doc.child(line_row(n, start, line, &spans, &carets, editor, dark));
        if !editor.completion_items().is_empty()
            && main_head >= start
            && main_head <= start + line.len()
        {
            doc = doc.child(completion_list(editor, ink, dark));
        }
    }
    let count = editor.cursor_count();
    let label = format!(
        "{count} cursor{} · document + selection, IME is not in the document",
        if count == 1 { "" } else { "s" }
    );
    col()
        .key("sql-editor")
        .w_full()
        .h(456.0)
        .gap(8.0)
        .child(
            row()
                .w_full()
                .h(28.0)
                .gap(12.0)
                .center_y()
                .child(text(label).font(rune::tok::type_::LABEL).color(ink).key("sql-cursor-count"))
                .child(
                    text("Add cursor")
                        .font(rune::tok::type_::LABEL)
                        .color(ink)
                        .key("sql-add-cursor")
                        .on_press(|state: &mut crate::AppState| state.sql.add_cursor_at_end()),
                )
                .child(
                    text(muted_note(editor))
                        .font(rune::tok::type_::LABEL)
                        .color(muted),
                ),
        )
        .child(
            row()
                .key("sql-find-bar")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text_input(&editor.find_input, editor_font())
                        .key("sql-find-input")
                        .w(150.0)
                        .h(24.0)
                        .on_edit(|state: &mut crate::AppState| &mut state.sql.find_input)
                        .on_submit(|state: &mut crate::AppState, tx| {
                            state.sql.find_from_bar();
                            tx.blur();
                        }),
                )
                .child(
                    text("Find next")
                        .font(rune::tok::type_::LABEL)
                        .color(ink)
                        .key("sql-find-next")
                        .on_press(|state: &mut crate::AppState| {
                            state.sql.find_from_bar();
                        }),
                )
                .child(
                    text_input(&editor.replace_input, editor_font())
                        .key("sql-replace-input")
                        .w(150.0)
                        .h(24.0)
                        .on_edit(|state: &mut crate::AppState| &mut state.sql.replace_input)
                        .on_submit(|state: &mut crate::AppState, tx| {
                            state.sql.replace_current_from_bar();
                            tx.blur();
                        }),
                )
                .child(
                    text("Replace")
                        .font(rune::tok::type_::LABEL)
                        .color(ink)
                        .key("sql-replace-current")
                        .on_press(|state: &mut crate::AppState| {
                            state.sql.replace_current_from_bar();
                        }),
                )
                .child(
                    text("Replace all")
                        .font(rune::tok::type_::LABEL)
                        .color(ink)
                        .key("sql-replace-all")
                        .on_press(|state: &mut crate::AppState| {
                            state.sql.replace_all_from_bar();
                        }),
                ),
        )
        .child(
            col()
                .key("sql-editor-scroll")
                .w_full()
                .h(380.0)
                .scroll_xy()
                .radius(8.0)
                .on_press(|state: &mut crate::AppState| {
                    // T1: pressing the editor surface hands the keyboard back
                    // from the result grid.
                    state.result_focused = false;
                    state.sql.focus();
                })
                .child(doc),
        )
}

/// The keyword completion, drawn in the canvas below the caret's line.
fn completion_list(editor: &SqlEditor, ink: rune::Color, dark: bool) -> El {
    let bg = if dark { oklch(0.22, 0.03, 250.0) } else { oklch(0.97, 0.01, 250.0) };
    let sel_bg = if dark { oklch(0.34, 0.08, 250.0) } else { oklch(0.86, 0.06, 250.0) };
    let mut list = col()
        .key("sql-completion")
        .w(170.0)
        .radius(6.0)
        .fill(bg);
    for (index, item) in editor.completion_items().iter().enumerate() {
        let selected = index == editor.completion_selected();
        list = list.child(
            row()
                .key(format!("sql-completion-item-{index}"))
                .w_full()
                .h(18.0)
                .px(8.0)
                .center_y()
                .fill(if selected { sel_bg } else { bg })
                .child(text(*item).font(editor_font()).color(ink))
                .on_press(move |state: &mut crate::AppState| {
                    state.sql.completion_accept_index(index);
                }),
        );
    }
    list
}

/// A plain document surface: the editor's model and IME caret, but no SQL
/// highlighting — for workflow YAML and other non-SQL text.
pub fn plain_panel(editor: &SqlEditor, ink: rune::Color, muted: rune::Color, dark: bool) -> El {
    let carets: Vec<(usize, bool)> = editor
        .selection
        .ranges()
        .iter()
        .enumerate()
        .map(|(i, sel)| (sel.head, i == editor.selection().main_index()))
        .collect();
    let mut doc = col().key("yaml-editor-field").w(EDITOR_SURFACE_WIDTH).min_h(160.0);
    for (n, (start, line)) in split_lines(editor.doc()).into_iter().enumerate() {
        doc = doc.child(line_row(n, start, line, &[], &carets, editor, dark));
    }
    col()
        .key("yaml-editor")
        .w_full()
        .h(220.0)
        .gap(4.0)
        .child(
            text("YAML · Command+Z undoes".to_owned())
                .font(rune::tok::type_::LABEL)
                .color(muted),
        )
        .child(
            col()
                .key("yaml-editor-scroll")
                .w_full()
                .h(180.0)
                .scroll_xy()
                .radius(8.0)
                .on_press(|state: &mut crate::AppState| state.workflow_yaml.focus())
                .child(doc),
        )
        .child(text(" ".to_owned()).w(0.0).h(0.0).color(ink))
}

fn muted_note(editor: &SqlEditor) -> String {
    match editor.composition() {
        Some(text) => format!("composing: {text}"),
        None => "composition draws on the main caret and commits into the document".to_owned(),
    }
}

fn line_row(
    index: usize,
    start: usize,
    line: &str,
    spans: &[Span],
    carets: &[(usize, bool)],
    editor: &SqlEditor,
    dark: bool,
) -> El {
    let end = start + line.len();
    let mut here: Vec<(usize, bool)> = carets
        .iter()
        .copied()
        .filter(|(at, _)| *at >= start && *at <= end)
        .collect();
    here.sort_by_key(|(at, _)| *at);
    here.dedup_by_key(|(at, _)| *at);
    let mut row = row().key(format!("sql-line-{index}")).h(18.0);
    let mut cursor = 0usize;
    for (at, is_main) in here {
        let rel = at - start;
        if rel > cursor {
            row = row.child(slice_text(line, start, cursor, rel, spans));
        }
        if is_main {
            if let Some(composed) = editor.composition() {
                let underline = if dark { oklch(0.8, 0.1, 95.0) } else { oklch(0.55, 0.12, 95.0) };
                row = row.child(
                    text(composed)
                        .font(editor_font())
                        .spans(vec![Span::new(0, composed.len()).underline(underline)]),
                );
            }
            if editor.focused() {
                row = row.child(caret(true, dark));
            }
        } else {
            row = row.child(caret(false, dark));
        }
        cursor = rel;
    }
    if cursor < line.len() {
        row = row.child(slice_text(line, start, cursor, line.len(), spans));
    }
    row
}

fn slice_text(line: &str, abs: usize, from: usize, to: usize, spans: &[Span]) -> El {
    let local: Vec<Span> = spans
        .iter()
        .filter_map(|span| clip_span(span, abs + from, abs + to, from))
        .collect();
    text(&line[from..to]).font(editor_font()).spans(local)
}

fn clip_span(span: &Span, abs_from: usize, abs_to: usize, origin: usize) -> Option<Span> {
    let lo = (span.start as usize).max(abs_from);
    let hi = (span.end as usize).min(abs_to);
    if lo >= hi {
        return None;
    }
    let mut out = Span::new(lo - abs_from + 0, hi - abs_from);
    let _ = origin;
    out.color = span.color;
    out.italic = span.italic;
    out.weight = span.weight;
    Some(out)
}

fn caret(main: bool, dark: bool) -> El {
    let fill = if dark {
        if main { oklch(0.9, 0.12, 250.0) } else { oklch(0.8, 0.08, 250.0) }
    } else if main {
        oklch(0.35, 0.10, 250.0)
    } else {
        oklch(0.45, 0.08, 250.0)
    };
    let bar = col().w(2.0).h(16.0).fill(fill);
    if main { bar.key("sql-caret").ime_cursor() } else { bar }
}

fn split_lines(doc: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, byte) in doc.bytes().enumerate() {
        if byte == b'\n' {
            out.push((start, &doc[start..i]));
            start = i + 1;
        }
    }
    out.push((start, &doc[start..]));
    out
}

fn keyword_color(dark: bool) -> rune::Color {
    if dark { oklch(0.72, 0.13, 305.0) } else { oklch(0.48, 0.16, 305.0) }
}
fn string_color(dark: bool) -> rune::Color {
    if dark { oklch(0.74, 0.12, 145.0) } else { oklch(0.50, 0.14, 145.0) }
}
fn comment_color(dark: bool) -> rune::Color {
    if dark { oklch(0.62, 0.02, 95.0) } else { oklch(0.52, 0.03, 95.0) }
}
fn number_color(dark: bool) -> rune::Color {
    if dark { oklch(0.78, 0.12, 70.0) } else { oklch(0.52, 0.14, 70.0) }
}
