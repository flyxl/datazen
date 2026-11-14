//! SQL editor shaped like CodeMirror 6, drawn by Rune.
//!
//! CodeMirror 6 keeps an immutable document and a selection (many ranges, one main). The
//! contenteditable node is only the input surface: plain characters arrive as key events, and
//! an IME composition is drawn at the main caret until it commits. This module does the same.
//! The document is a `String`. Rune does not edit it. `on_key` and `on_ime` build changes, and
//! the view paints lines, highlight spans and carets. The main caret is an `El::ime_cursor`,
//! which is what turns the platform input method on.

use rune::{El, Ime, Key, KeyEvent, Span, TextEdit, TextStyle, col, oklch, row, text, text_input};

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

/// One undo step: the document and selection before a change.
#[derive(Clone, Debug)]
struct Snapshot {
    doc: String,
    selection: Selection,
}

/// How many insert and delete steps the editor remembers.
const HISTORY_LIMIT: usize = 100;

#[derive(Debug)]
pub struct SqlEditor {
    doc: String,
    selection: Selection,
    /// Marked text at the main caret. Not part of `doc` until commit.
    composition: Option<String>,
    focused: bool,
    /// Undo and redo cover character insertion and deletion only.
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    /// The find bar's needle and replacement.
    pub(crate) find_input: TextEdit,
    pub(crate) replace_input: TextEdit,
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
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            find_input: TextEdit::new(String::new()),
            replace_input: TextEdit::new(String::new()),
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
        self.push_history();
        self.change_by_range(|sel| (sel.from(), sel.to(), text.to_owned()));
    }

    pub fn backspace(&mut self) {
        if self.composition.is_some() {
            return;
        }
        self.push_history();
        let doc = self.doc.clone();
        self.change_by_range(|sel| {
            if sel.from() != sel.to() {
                (sel.from(), sel.to(), String::new())
            } else {
                (prev_boundary(&doc, sel.head), sel.head, String::new())
            }
        });
    }

    /// Undo the last insert or delete; false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        match self.undo_stack.pop() {
            Some(snapshot) => {
                self.redo_stack.push(Snapshot {
                    doc: std::mem::take(&mut self.doc),
                    selection: self.selection.clone(),
                });
                self.doc = snapshot.doc;
                self.selection = snapshot.selection;
                true
            }
            None => false,
        }
    }

    /// Redo the last undone insert or delete; false when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        match self.redo_stack.pop() {
            Some(snapshot) => {
                self.undo_stack.push(Snapshot {
                    doc: std::mem::take(&mut self.doc),
                    selection: self.selection.clone(),
                });
                self.doc = snapshot.doc;
                self.selection = snapshot.selection;
                true
            }
            None => false,
        }
    }

    /// Split the document into statements at semicolons: each non-empty segment's
    /// byte range, without its terminating semicolon. Empty segments are skipped.
    pub fn statements(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = 0;
        for (i, byte) in self.doc.bytes().enumerate() {
            if byte == b';' {
                if !self.doc[start..i].trim().is_empty() {
                    out.push((start, i));
                }
                start = i + 1;
            }
        }
        if !self.doc[start..].trim().is_empty() {
            out.push((start, self.doc.len()));
        }
        out
    }

    /// The statement the main caret is in: the first statement ending at or after
    /// the caret. A caret right after a semicolon picks the next statement; a
    /// caret past the last statement picks the last one. None when the document
    /// holds no statement.
    pub fn statement_at_cursor(&self) -> Option<String> {
        let head = self.selection.main().head;
        let statements = self.statements();
        statements
            .iter()
            .find(|(_, end)| head <= *end)
            .or_else(|| statements.last())
            .map(|(start, end)| self.doc[*start..*end].trim().to_owned())
    }

    /// Find the next occurrence of `needle` from the main caret, wrapping around,
    /// and select it. False when the needle is missing or empty.
    pub fn find_next(&mut self, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        let main = self.selection.main();
        let start = if main.from() != main.to() && self.doc.get(main.from()..main.to()) == Some(needle) {
            main.to()
        } else {
            main.head
        };
        let found = self.doc[start..]
            .find(needle)
            .map(|i| start + i)
            .or_else(|| self.doc[..start].find(needle));
        match found {
            Some(at) => {
                self.selection = Selection {
                    ranges: vec![Sel { anchor: at, head: at + needle.len() }],
                    main: 0,
                };
                true
            }
            None => false,
        }
    }

    /// Replace the main selection when it matches `needle`, then select the next
    /// occurrence. False when the selection is not a match.
    pub fn replace_current(&mut self, needle: &str, replacement: &str) -> bool {
        let main = self.selection.main();
        if needle.is_empty() || main.from() == main.to() {
            return false;
        }
        if self.doc.get(main.from()..main.to()) != Some(needle) {
            return false;
        }
        self.push_history();
        self.apply_change(main.from(), main.to(), replacement);
        self.find_next(needle);
        true
    }

    /// Replace every occurrence of `needle`; returns how many landed.
    pub fn replace_all(&mut self, needle: &str, replacement: &str) -> usize {
        if needle.is_empty() {
            return 0;
        }
        let mut positions = Vec::new();
        let mut at = 0;
        while let Some(i) = self.doc[at..].find(needle) {
            positions.push(at + i);
            at = at + i + needle.len();
        }
        if positions.is_empty() {
            return 0;
        }
        self.push_history();
        for from in positions.iter().rev() {
            self.apply_change(*from, from + needle.len(), replacement);
        }
        positions.len()
    }

    /// The find bar's Find: read the bar's needle.
    pub fn find_from_bar(&mut self) -> bool {
        let needle = self.find_input.text.clone();
        self.find_next(&needle)
    }

    /// The find bar's Replace: read the bar's needle and replacement.
    pub fn replace_current_from_bar(&mut self) -> bool {
        let needle = self.find_input.text.clone();
        let replacement = self.replace_input.text.clone();
        self.replace_current(&needle, &replacement)
    }

    /// The find bar's Replace all.
    pub fn replace_all_from_bar(&mut self) -> usize {
        let needle = self.find_input.text.clone();
        let replacement = self.replace_input.text.clone();
        self.replace_all(&needle, &replacement)
    }

    fn push_history(&mut self) {
        if self.undo_stack.len() >= HISTORY_LIMIT {
            self.undo_stack.remove(0);
        }
        self.undo_stack
            .push(Snapshot { doc: self.doc.clone(), selection: self.selection.clone() });
        self.redo_stack.clear();
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
        Key::Command('z') if ev.mods.shift => {
            editor.redo();
            true
        }
        Key::Command('z') => {
            editor.undo();
            true
        }
        Key::Command('y') => {
            editor.redo();
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
        assert!(on_key(&mut editor, &KeyEvent { key: Key::Text("c".into()), mods: rune::Mods::default() }));
        assert_eq!(editor.doc(), "cab");
        assert!(on_key(&mut editor, &KeyEvent { key: Key::Command('z'), mods: rune::Mods::default() }));
        assert_eq!(editor.doc(), "ab");
        assert!(on_key(
            &mut editor,
            &KeyEvent { key: Key::Command('z'), mods: rune::Mods { shift: true, ..rune::Mods::default() } }
        ));
        assert_eq!(editor.doc(), "cab", "shift-command-z redoes");
        assert!(on_key(&mut editor, &KeyEvent { key: Key::Command('y'), mods: rune::Mods::default() }) == false
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
    fn typing_is_ignored_while_a_composition_is_open() {
        let mut editor = SqlEditor::from_doc("ab");
        editor.focus();
        editor.preedit("中");
        assert!(!on_key(&mut editor, &KeyEvent { key: Key::Text("A".into()), mods: rune::Mods::default() }));
        assert_eq!(editor.doc(), "ab");
    }
}
