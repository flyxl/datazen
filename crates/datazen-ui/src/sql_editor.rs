//! SQL editor shaped like CodeMirror 6, drawn by Rune.
//!
//! CodeMirror 6 keeps an immutable document and a selection (many ranges, one main). The
//! contenteditable node is only the input surface: plain characters arrive as key events, and
//! an IME composition is drawn at the main caret until it commits. This module does the same.
//! The document is a `String`. Rune does not edit it. `on_key` and `on_ime` build changes, and
//! the view paints lines, highlight spans and carets. The main caret is an `El::ime_cursor`,
//! which is what turns the platform input method on.

use rune::{El, Ime, Key, KeyEvent, Span, TextStyle, col, oklch, row, text};

/// Editing surface width. The scroll viewport is narrower, so a long line moves sideways
/// instead of wrapping. CodeMirror's `lineWrapping` off does this inside the line layout;
/// Rune's multi-line field cannot turn wrapping off, so the view is not a text field.
pub const EDITOR_SURFACE_WIDTH: f32 = 2800.0;

const KEYWORDS: &[&str] = &[
    "ALL", "ALTER", "AND", "AS", "AVG", "BY", "CASE", "COUNT", "CREATE", "DELETE", "DISTINCT",
    "DROP", "ELSE", "END", "FOREIGN", "FROM", "GROUP", "IN", "INDEX", "INNER", "INSERT", "INTO",
    "IS", "JOIN", "KEY", "LEFT", "LIMIT", "MAX", "MIN", "NOT", "NULL", "OFFSET", "ON", "OR",
    "ORDER", "PRIMARY", "REFERENCES", "RIGHT", "SELECT", "SET", "SUM", "TABLE", "THEN", "UPDATE",
    "VALUES", "WHEN", "WHERE", "WITH",
];

/// One selection range. `head` is the caret. `anchor` is the other end. Equal means a caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sel {
    pub anchor: usize,
    pub head: usize,
}

impl Sel {
    pub fn caret(at: usize) -> Self {
        Self { anchor: at, head: at }
    }

    pub fn from(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn to(&self) -> usize {
        self.anchor.max(self.head)
    }
}

/// CodeMirror's `EditorSelection`: ranges plus which one is main.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    ranges: Vec<Sel>,
    main: usize,
}

impl Selection {
    pub fn single(at: usize) -> Self {
        Self { ranges: vec![Sel::caret(at)], main: 0 }
    }

    pub fn main(&self) -> Sel {
        self.ranges[self.main]
    }

    pub fn ranges(&self) -> &[Sel] {
        &self.ranges
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }
}

#[derive(Debug)]
pub struct SqlEditor {
    doc: String,
    selection: Selection,
    /// Marked text at the main caret. Not part of `doc` until commit.
    composition: Option<String>,
    focused: bool,
}

impl Default for SqlEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl SqlEditor {
    pub fn new() -> Self {
        let long = "x".repeat(180);
        let sample = format!(
            "-- customer lookup\nSELECT id, name FROM customers WHERE status = 'active' AND note = '{long}';\n"
        );
        Self::from_doc(sample)
    }

    pub fn from_doc(doc: impl Into<String>) -> Self {
        Self {
            doc: doc.into(),
            selection: Selection::single(0),
            composition: None,
            focused: false,
        }
    }

    pub fn doc(&self) -> &str {
        &self.doc
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn composition(&self) -> Option<&str> {
        self.composition.as_deref()
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn focus(&mut self) {
        self.focused = true;
    }

    pub fn cursor_count(&self) -> usize {
        self.selection.len()
    }

    pub fn main_head(&self) -> usize {
        self.selection.main().head
    }

    /// Extra carets, not the main one. Byte offsets into `doc`.
    pub fn extra_carets(&self) -> Vec<usize> {
        self.selection
            .ranges
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != self.selection.main)
            .map(|(_, sel)| sel.head)
            .collect()
    }

    /// CodeMirror `addCursor`: a new caret, and it becomes the main range.
    pub fn add_cursor(&mut self, byte: usize) {
        let at = boundary(&self.doc, byte);
        if let Some(i) = self.selection.ranges.iter().position(|r| r.anchor == at && r.head == at) {
            self.selection.main = i;
            return;
        }
        self.selection.ranges.push(Sel::caret(at));
        self.selection.main = self.selection.ranges.len() - 1;
    }

    pub fn add_cursor_at_end(&mut self) {
        self.add_cursor(self.doc.len());
    }

    /// Same insertion at every range (`changeByRange`). A composition owns the caret, so
    /// typing does not also go into the document.
    pub fn insert(&mut self, text: &str) {
        if self.composition.is_some() || text.is_empty() {
            return;
        }
        self.change_by_range(|sel| (sel.from(), sel.to(), text.to_owned()));
    }

    pub fn backspace(&mut self) {
        if self.composition.is_some() {
            return;
        }
        let doc = self.doc.clone();
        self.change_by_range(|sel| {
            if sel.from() != sel.to() {
                (sel.from(), sel.to(), String::new())
            } else {
                (prev_boundary(&doc, sel.head), sel.head, String::new())
            }
        });
    }

    /// `Ime::Preedit`. Empty ends the composition and leaves the document alone.
    pub fn preedit(&mut self, text: &str) {
        self.composition = if text.is_empty() { None } else { Some(text.to_owned()) };
    }

    /// `Ime::Commit`: the composed string replaces the main range only. Other ranges are mapped
    /// through that change, which is what CodeMirror does when the input method updates the DOM.
    pub fn commit(&mut self, text: &str) {
        self.composition = None;
        let main = self.selection.main();
        self.apply_change(main.from(), main.to(), text);
    }

    pub fn on_ime(&mut self, ev: &Ime) -> bool {
        if !self.focused {
            return false;
        }
        match ev {
            Ime::Preedit(text, _) => self.preedit(text),
            Ime::Commit(text) => self.commit(text),
            Ime::Enabled | Ime::Disabled => {}
        }
        true
    }

    fn change_by_range(&mut self, mut change: impl FnMut(Sel) -> (usize, usize, String)) {
        let snapshot = self.selection.ranges.clone();
        let main_i = self.selection.main;
        let mut specs: Vec<(usize, usize, String, bool)> = snapshot
            .iter()
            .enumerate()
            .map(|(i, sel)| {
                let (from, to, text) = change(*sel);
                (from, to, text, i == main_i)
            })
            .collect();
        specs.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        let mut built: Vec<(bool, Sel)> = Vec::new();
        for (from, to, text, is_main) in specs {
            let from = boundary(&self.doc, from);
            let to = boundary(&self.doc, to.max(from));
            let end = from + text.len();
            self.doc.replace_range(from..to, &text);
            for (placed_main, sel) in &mut built {
                let _ = placed_main;
                let shift = text.len() as isize - (to - from) as isize;
                if sel.head >= to {
                    sel.head = (sel.head as isize + shift) as usize;
                    sel.anchor = (sel.anchor as isize + shift) as usize;
                }
            }
            built.push((is_main, Sel::caret(end)));
        }
        let main = built.iter().position(|(is_main, _)| *is_main).unwrap_or(0);
        self.selection.ranges = built.into_iter().map(|(_, sel)| sel).collect();
        self.selection.main = main;
    }

    fn apply_change(&mut self, from: usize, to: usize, text: &str) {
        let from = boundary(&self.doc, from);
        let to = boundary(&self.doc, to.max(from));
        let new_len = text.len();
        self.doc.replace_range(from..to, text);
        let map = |pos: usize| -> usize {
            if pos < from {
                pos
            } else if pos > to {
                pos - (to - from) + new_len
            } else {
                from + new_len
            }
        };
        for sel in &mut self.selection.ranges {
            sel.anchor = map(sel.anchor);
            sel.head = map(sel.head);
        }
    }
}

pub fn on_key(editor: &mut SqlEditor, ev: &KeyEvent) -> bool {
    if !editor.focused() || editor.composition().is_some() {
        return false;
    }
    match &ev.key {
        Key::Text(text) if !ev.mods.cmd && !ev.mods.ctrl => {
            editor.insert(text);
            true
        }
        Key::Enter => {
            editor.insert("\n");
            true
        }
        Key::Backspace => {
            editor.backspace();
            true
        }
        _ => false,
    }
}

pub fn on_ime(editor: &mut SqlEditor, ev: &Ime) -> bool {
    editor.on_ime(ev)
}

pub fn editor_font() -> TextStyle {
    TextStyle::new(13.0, 400.0)
}

pub fn highlight_sql(src: &str) -> Vec<Span> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut spans = Vec::new();
    while i < bytes.len() {
        if bytes[i] == b'-' && bytes.get(i + 1) == Some(&b'-') {
            let start = i;
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            spans.push(Span::new(start, i).color(comment_color()).italic());
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            spans.push(Span::new(start, i).color(comment_color()).italic());
            continue;
        }
        if bytes[i] == b'\'' || bytes[i] == b'"' {
            let quote = bytes[i];
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == quote {
                    i += 1;
                    if bytes.get(i) == Some(&quote) {
                        i += 1;
                        continue;
                    }
                    break;
                }
                if bytes[i] == b'\\' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            spans.push(Span::new(start, i).color(string_color()));
            continue;
        }
        if bytes[i].is_ascii_digit() {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            spans.push(Span::new(start, i).color(number_color()));
            continue;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if is_keyword(&src[start..i]) {
                spans.push(Span::new(start, i).color(keyword_color()).bold());
            }
            continue;
        }
        i += 1;
    }
    spans
}

/// The editor page: a scroll view of unwrapped lines. Not a `text_input`.
pub fn panel(editor: &SqlEditor, ink: rune::Color, muted: rune::Color) -> El {
    let spans = highlight_sql(&editor.doc);
    let carets: Vec<(usize, bool)> = editor
        .selection
        .ranges()
        .iter()
        .enumerate()
        .map(|(i, sel)| (sel.head, i == editor.selection.main))
        .collect();
    let mut doc = col().key("sql-editor-field").w(EDITOR_SURFACE_WIDTH).min_h(300.0);
    for (n, (start, line)) in split_lines(&editor.doc).into_iter().enumerate() {
        doc = doc.child(line_row(n, start, line, &spans, &carets, editor));
    }
    let count = editor.cursor_count();
    let label = format!(
        "{count} cursor{} · document + selection, IME is not in the document",
        if count == 1 { "" } else { "s" }
    );
    col()
        .key("sql-editor")
        .w_full()
        .h(420.0)
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
            col()
                .key("sql-editor-scroll")
                .w_full()
                .h(380.0)
                .scroll_xy()
                .radius(8.0)
                .on_press(|state: &mut crate::AppState| state.sql.focus())
                .child(doc),
        )
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
                row = row.child(
                    text(composed)
                        .font(editor_font())
                        .spans(vec![Span::new(0, composed.len()).underline(oklch(0.8, 0.1, 95.0))]),
                );
            }
            if editor.focused() {
                row = row.child(caret(true));
            }
        } else {
            row = row.child(caret(false));
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

fn caret(main: bool) -> El {
    let bar = col().w(2.0).h(16.0).fill(if main { oklch(0.9, 0.12, 250.0) } else { oklch(0.8, 0.08, 250.0) });
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

fn keyword_color() -> rune::Color {
    oklch(0.72, 0.13, 305.0)
}
fn string_color() -> rune::Color {
    oklch(0.74, 0.12, 145.0)
}
fn comment_color() -> rune::Color {
    oklch(0.62, 0.02, 95.0)
}
fn number_color() -> rune::Color {
    oklch(0.78, 0.12, 70.0)
}

fn is_keyword(word: &str) -> bool {
    KEYWORDS.binary_search_by(|k| cmp_ignore_ascii_case(k, word)).is_ok()
}

fn cmp_ignore_ascii_case(a: &str, b: &str) -> std::cmp::Ordering {
    a.as_bytes()
        .iter()
        .map(|c| c.to_ascii_uppercase())
        .cmp(b.as_bytes().iter().map(|c| c.to_ascii_uppercase()))
}

fn boundary(text: &str, byte: usize) -> usize {
    text.floor_char_boundary(byte.min(text.len()))
}

fn prev_boundary(text: &str, byte: usize) -> usize {
    if byte == 0 {
        return 0;
    }
    let mut i = byte - 1;
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_table_is_sorted() {
        let mut sorted = KEYWORDS.to_vec();
        sorted.sort_unstable();
        assert_eq!(KEYWORDS, sorted.as_slice());
    }

    #[test]
    fn highlights_keywords_strings_comments_and_numbers() {
        let src = "SELECT 42 FROM t WHERE name = '雪' -- tail\n";
        let spans = highlight_sql(src);
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
        let spans = highlight_sql(src);
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
    fn typing_is_ignored_while_a_composition_is_open() {
        let mut editor = SqlEditor::from_doc("ab");
        editor.focus();
        editor.preedit("中");
        assert!(!on_key(&mut editor, &KeyEvent { key: Key::Text("A".into()), mods: rune::Mods::default() }));
        assert_eq!(editor.doc(), "ab");
    }
}
