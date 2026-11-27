use rune::{El, Ime, Key, KeyEvent, Span, TextEdit, TextStyle, Tx, col, row, text};

use crate::i18n::t;
use crate::ui::palette;

use super::highlight::highlight_with_palette;
use super::syntax_theme::SyntaxPalette;
use super::model::SqlEditor;
use crate::ui::field::TextField;

/// The current Native editing surface scrolls long lines horizontally.
/// React enables soft wrap; it needs shared render/caret geometry and is not
/// approximated by independently wrapping the text fragments here.
pub const EDITOR_SURFACE_WIDTH: f32 = 2800.0;
const SQL_BODY_PADDING: f32 = 12.0;
const DEFAULT_SIZE: f32 = datazen_ui_api::DEFAULT_EDITOR_FONT_SIZE as f32;

pub fn editor_font() -> TextStyle {
    TextStyle::new(13.0, 400.0)
}

pub(crate) fn query_font_family(family: datazen_ui_api::EditorFontFamily) -> rune::tok::Family {
    use datazen_ui_api::EditorFontFamily::*;
    match family {
        Bundled => rune::tok::Family::Mono,
        System => rune::tok::Family::SystemMono,
    }
}

/// The editor page: a scroll view of unwrapped lines. Not a `text_input`.
pub fn panel(editor: &SqlEditor, ink: rune::Color, muted: rune::Color, dark: bool) -> El {
    panel_with_completion_scope(
        editor, ink, muted, dark, editor.completion_scope().is_none(), None,
        datazen_ui_api::DEFAULT_EDITOR_FONT_SIZE,
        datazen_ui_api::EditorFontFamily::Bundled,
        datazen_ui_api::SqlSyntaxTheme::DEFAULT,
    )
}

pub(crate) fn panel_with_completion_scope(
    editor: &SqlEditor, ink: rune::Color, muted: rune::Color, dark: bool,
    completion_current: bool,
    viewport: Option<crate::query_editor_tabs::QueryEditorViewport>,
    font_size: u32,
    font_family: datazen_ui_api::EditorFontFamily,
    syntax_theme: datazen_ui_api::SqlSyntaxTheme,
) -> El {
    let size = font_size as f32;
    let row_height = size * 1.4;
    let caret_height = size * (16.0 / DEFAULT_SIZE);
    let completion_height = size * (18.0 / DEFAULT_SIZE);
    let font = TextStyle::new(size, 400.0).family(query_font_family(font_family));
    let syntax = SyntaxPalette::live(syntax_theme, dark);
    let spans = highlight_with_palette(editor.doc(), syntax);
    let document_ink = syntax.foreground.unwrap_or(ink);
    let carets: Vec<(usize, bool)> = editor
        .selection
        .ranges()
        .iter()
        .enumerate()
        .map(|(i, sel)| (sel.head, i == editor.selection().main_index()))
        .collect();
    let main_head = editor.main_head();
    let lines = split_lines(editor.doc());
    let gutter_width =
        13.0 + size * (8.0 / DEFAULT_SIZE) * (lines.len().max(1).ilog10() + 1) as f32;
    let (gutter_bg, gutter_ink, gutter_edge) = palette::sql_editor_gutter(dark);
    let mut gutter = col()
        .key("sql-editor-gutter")
        .w(gutter_width)
        .py(SQL_BODY_PADDING)
        .sticky_left(0.0)
        .fill(gutter_bg)
        .border_sides([0.0, 1.0, 0.0, 0.0], gutter_edge);
    let mut doc = col()
        .key("sql-editor-field")
        .w(EDITOR_SURFACE_WIDTH)
        .min_h(300.0)
        .py(SQL_BODY_PADDING);
    for (n, (start, line)) in lines.into_iter().enumerate() {
        gutter = gutter.child(
            row()
                .key(format!("sql-gutter-line-{n}"))
                .h(row_height)
                .w_full()
                .px(6.0)
                .center_y()
                .justify_end()
                .child(
                    text((n + 1).to_string())
                        .key(format!("sql-gutter-number-{n}"))
                        .font(font)
                        .color(gutter_ink),
                ),
        );
        doc = doc.child(line_row_with_style(
            n, start, line, &spans, &carets, editor, dark,
            LineStyle {
                font, height: row_height, caret_height, ink: Some(document_ink),
                selection: syntax.selection,
            },
        ));
        if completion_current && !editor.completion_items().is_empty()
            && main_head >= start
            && main_head <= start + line.len()
        {
            gutter = gutter.child(col().h(completion_height * editor.completion_items().len() as f32));
            doc = doc.child(completion_list(editor, ink, dark, size, completion_height));
        }
    }
    let sheet = row().key("sql-editor-sheet").child(gutter).child(doc);
    let (doc, scroll) = match viewport {
        Some(viewport) => (
            col().key(viewport.anchor).child(sheet),
            Some(viewport.offset),
        ),
        None => (sheet, None),
    };
    col()
        .key("sql-editor")
        .w_full()
        .h_full()
        .min_h(120.0)
        .gap(8.0)
        .child(
            row()
                .key("sql-find-bar")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    TextField::new("sql-find-input", &editor.find_input)
                        .height(24.0)
                        .text_style(editor_font())
                        .width(150.0)
                        .on_edit(|state: &mut crate::AppState| &mut state.sql.find_input)
                        .on_submit(|state: &mut crate::AppState, tx| {
                            state.sql.find_from_bar();
                            tx.blur();
                        })
                        .build(dark),
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
                    TextField::new("sql-replace-input", &editor.replace_input)
                        .height(24.0)
                        .text_style(editor_font())
                        .width(150.0)
                        .on_edit(|state: &mut crate::AppState| &mut state.sql.replace_input)
                        .on_submit(|state: &mut crate::AppState, tx| {
                            state.sql.replace_current_from_bar();
                            tx.blur();
                        })
                        .build(dark),
                )
                .child(
                    text(t("resultFind.replaceNext"))
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
                )
                .child(
                    text("Add cursor")
                        .font(rune::tok::type_::LABEL)
                        .color(muted)
                        .key("sql-add-cursor")
                        .on_press(|state: &mut crate::AppState| state.sql.add_cursor_at_end()),
                ),
        )
        .child(
            {
                let body = col()
                    .key("sql-editor-body")
                    .w_full()
                    .grow(1.0)
                    .min_h(60.0);
                if let Some(background) = syntax.background { body.fill(background) } else { body }
            }
                // The numbered sheet can be shorter than the idle canvas.
                // Keep the gutter chrome visible to the viewport's bottom
                // without padding the document or changing scroll offsets.
                .child(
                    col()
                        .key("sql-editor-gutter-track")
                        .pin(Some(0.0), Some(0.0), None, Some(0.0))
                        .w(gutter_width)
                        .fill(gutter_bg)
                        .border_sides([0.0, 1.0, 0.0, 0.0], gutter_edge),
                )
                .child(
                    col()
                        .key("sql-editor-scroll")
                        .w_full()
                        .grow(1.0)
                        .min_h(0.0)
                        .scroll_xy()
                        .on_scroll_offset(move |at| {
                            if let Some(scroll) = &scroll { scroll.set(at.offset); }
                        })
                        .on_press(|state: &mut crate::AppState| {
                            // The editor takes keyboard input back from the result grid.
                            state.result_focused = false;
                            state.sql.focus();
                        })
                        .child(doc),
                ),
        )
}

/// The completion list, drawn in the canvas below the caret line.
fn completion_list(
    editor: &SqlEditor, ink: rune::Color, dark: bool, size: f32, row_height: f32,
) -> El {
    let bg = palette::editor_bg(dark);
    let sel_bg = palette::editor_selection_bg(dark);
    let mut list = col()
        .key("sql-completion")
        .w(if editor.completion_scope().is_some() { 360.0 } else { 170.0 })
        .radius(6.0)
        .fill(bg);
    for (index, item) in editor.completion_items().iter().enumerate() {
        let selected = index == editor.completion_selected();
        let scope = editor.completion_scope().cloned();
        let label = item.clone();
        list = list.child(
            row()
                .key(format!("sql-completion-item-{index}"))
                .w_full()
                .h(row_height)
                .px(8.0)
                .center_y()
                .fill(if selected { sel_bg } else { bg })
                .child(text(item.clone()).font(TextStyle::new(size, 400.0)).color(ink))
                .on_press(move |state: &mut crate::AppState| {
                    if state.catalog_completion_current()
                        && state.sql.completion_scope() == scope.as_ref()
                        && state.sql.completion_items().get(index) == Some(&label)
                    {
                        state.sql.completion_accept_index(index);
                    }
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

#[derive(Clone, Copy)]
struct LineStyle {
    font: TextStyle,
    height: f32,
    caret_height: f32,
    ink: Option<rune::Color>,
    selection: Option<rune::Color>,
}

pub fn line_row(
    index: usize,
    start: usize,
    line: &str,
    spans: &[Span],
    carets: &[(usize, bool)],
    editor: &SqlEditor,
    dark: bool,
) -> El {
    line_row_with_style(index, start, line, spans, carets, editor, dark,
        LineStyle { font: editor_font(), height: 18.0, caret_height: 16.0, ink: None, selection: None })
}

fn line_row_with_style(
    index: usize,
    start: usize,
    line: &str,
    spans: &[Span],
    carets: &[(usize, bool)],
    editor: &SqlEditor,
    dark: bool,
    style: LineStyle,
) -> El {
    let end = start + line.len();
    let mut here: Vec<(usize, bool)> = carets
        .iter()
        .copied()
        .filter(|(at, _)| *at >= start && *at <= end)
        .collect();
    here.sort_by_key(|(at, _)| *at);
    here.dedup_by_key(|(at, _)| *at);
    let mut row = row().key(format!("sql-line-{index}")).h(style.height);
    if style.ink.is_some() {
        row = row.center_y();
    }
    let selected = editor.selection().ranges();
    let append = |mut row: El, from: usize, to: usize| {
        let mut edges = vec![from, to];
        for sel in selected {
            for edge in [sel.from(), sel.to()].into_iter().filter_map(|edge| edge.checked_sub(start)) {
                if edge > from && edge < to && line.is_char_boundary(edge) {
                    edges.push(edge);
                }
            }
        }
        edges.sort_unstable();
        edges.dedup();
        for pair in edges.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            let mut fragment = slice_text(line, start, a, b, spans, style);
            if selected.iter().any(|sel| sel.from() <= start + a && start + b <= sel.to()) {
                fragment = fragment
                    .key(format!("sql-selected-{index}-{a}"))
                    .fill(style.selection.unwrap_or_else(|| palette::editor_selection_bg(dark)));
            }
            row = row.child(fragment);
        }
        row
    };
    let mut cursor = 0usize;
    for (at, is_main) in here {
        let rel = at - start;
        if rel > cursor {
            row = append(row, cursor, rel);
        }
        if is_main {
            if let Some(composed) = editor.composition() {
                let underline = palette::editor_composition_underline(dark);
                row = row.child(
                    text(composed)
                        .font(style.font)
                        .spans(vec![Span::new(0, composed.len()).underline(underline)]),
                );
            }
            if editor.focused() {
                row = row.child(caret(true, dark, style.caret_height));
            }
        } else {
            row = row.child(caret(false, dark, style.caret_height));
        }
        cursor = rel;
    }
    if cursor < line.len() {
        row = append(row, cursor, line.len());
    }
    row
}

fn slice_text(line: &str, abs: usize, from: usize, to: usize, spans: &[Span], style: LineStyle) -> El {
    let local: Vec<Span> = spans
        .iter()
        .filter_map(|span| clip_span(span, abs + from, abs + to, from))
        .collect();
    let fragment = text(&line[from..to]).font(style.font).spans(local);
    match style.ink {
        Some(ink) => fragment.color(ink),
        None => fragment,
    }
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

fn caret(main: bool, dark: bool, height: f32) -> El {
    let fill = palette::editor_caret_fill(main, dark);
    let bar = col().w(2.0).h(height).fill(fill);
    if main { bar.key("sql-caret").ime_cursor() } else { bar }
}

/// Split a document into `(byte_offset, line)` pairs for row rendering.
/// Crate-visible for the workflow yaml editor's surface (T200), which
/// mounts the same rows the plain panel renders.
pub fn split_lines(doc: &str) -> Vec<(usize, &str)> {
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
    palette::code_palette(dark).keyword
}
fn string_color(dark: bool) -> rune::Color {
    palette::code_palette(dark).string
}
fn comment_color(dark: bool) -> rune::Color {
    palette::code_palette(dark).comment
}
fn number_color(dark: bool) -> rune::Color {
    palette::code_palette(dark).number
}
