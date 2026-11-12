//! SQL Editor proof of concept.
//!
//! Rune's `TextEdit` is one caret plus an IME composition. This module keeps that caret as
//! the primary cursor (so composition, undo and the field's own keys keep working) and stores
//! any extra carets beside it. Typing with extra carets inserts the same text at each of them.
//!
//! Highlighting is a small SQL lexer painted with `Span`s. It is not a parser.
//!
//! Long lines do not wrap at the viewport: the field is a fixed wide surface inside a
//! bidirectional scroll view, which is the stand-in for CodeMirror's nowrap plus horizontal
//! scroll until Rune exposes nowrap on a multi-line field.

use rune::{Ime, Key, KeyEvent, Mods, Span, TextEdit, TextStyle, oklch};

/// Width of the editing surface, wider than the surrounding viewport so a long line scrolls
/// horizontally instead of wrapping at the pane edge.
pub const EDITOR_SURFACE_WIDTH: f32 = 2800.0;

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

const KEYWORDS: &[&str] = &[
    "ALL", "ALTER", "AND", "AS", "AVG", "BY", "CASE", "COUNT", "CREATE", "DELETE", "DISTINCT",
    "DROP", "ELSE", "END", "FOREIGN", "FROM", "GROUP", "IN", "INDEX", "INNER", "INSERT", "INTO",
    "IS", "JOIN", "KEY", "LEFT", "LIMIT", "MAX", "MIN", "NOT", "NULL", "OFFSET", "ON", "OR",
    "ORDER", "PRIMARY", "REFERENCES", "RIGHT", "SELECT", "SET", "SUM", "TABLE", "THEN", "UPDATE",
    "VALUES", "WHEN", "WHERE", "WITH",
];

#[derive(Debug)]
pub struct SqlEditor {
    pub edit: TextEdit,
    /// Carets other than `edit.focus`, as byte offsets into `edit.text`.
    extra: Vec<usize>,
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
        Self { edit: TextEdit::new(sample), extra: Vec::new() }
    }

    pub fn cursor_count(&self) -> usize {
        1 + self.extra.len()
    }

    pub fn extra_carets(&self) -> &[usize] {
        &self.extra
    }

    /// Add a caret at `byte` (clamped to a char boundary). The primary caret is unchanged.
    pub fn add_cursor(&mut self, byte: usize) {
        let at = boundary(&self.edit.text, byte);
        if at != self.edit.focus && !self.extra.contains(&at) {
            self.extra.push(at);
        }
    }

    pub fn add_cursor_at_end(&mut self) {
        self.add_cursor(self.edit.text.len());
    }

    /// Insert `text` at the primary caret and every extra caret, from right to left so earlier
    /// offsets stay valid. The primary selection is collapsed to its focus first.
    pub fn insert_all(&mut self, text: &str) {
        let primary = boundary(&self.edit.text, self.edit.focus);
        let mut carets = self.extra.clone();
        carets.push(primary);
        carets.sort_unstable();
        carets.dedup();
        let len = text.len();
        let mut updated_extra = Vec::new();
        let mut updated_primary = primary;
        let mut placed: Vec<(bool, usize)> = Vec::new();
        for caret in carets.into_iter().rev() {
            let caret = boundary(&self.edit.text, caret);
            self.edit.anchor = caret;
            self.edit.focus = caret;
            self.edit.insert(text);
            for (_, pos) in &mut placed {
                *pos += len;
            }
            placed.push((caret == primary, caret + len));
        }
        for (is_primary, at) in placed {
            if is_primary {
                updated_primary = at;
            } else {
                updated_extra.push(at);
            }
        }
        self.edit.anchor = updated_primary;
        self.edit.focus = updated_primary;
        self.extra = updated_extra;
        self.clamp_carets();
    }

    pub fn highlight_spans(&self) -> Vec<Span> {
        highlight_sql(&self.edit.text)
    }

    /// Drop carets the latest edit left off a char boundary or past the end.
    pub fn clamp_carets(&mut self) {
        let n = self.edit.text.len();
        let primary = self.edit.focus;
        self.extra.retain(|&c| c <= n && self.edit.text.is_char_boundary(c) && c != primary);
    }
}

pub fn editor_font() -> TextStyle {
    TextStyle::new(13.0, 400.0)
}

/// Consume a text key when extra carets are active, inserting it at every caret.
/// A lone primary caret returns false so Rune's field (and IME) handles the key.
pub fn on_editor_key(editor: &mut SqlEditor, ev: &KeyEvent) -> bool {
    if editor.extra.is_empty() || ev.mods.cmd || ev.mods.ctrl {
        return false;
    }
    let Key::Text(text) = &ev.key else { return false };
    editor.insert_all(text);
    true
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

/// Helpers re-exported for tests that drive a focused field.
pub fn ime_preedit(text: impl Into<String>) -> Ime {
    Ime::Preedit(text.into(), None)
}

pub fn ime_commit(text: impl Into<String>) -> Ime {
    Ime::Commit(text.into())
}

pub fn text_key(text: &str) -> KeyEvent {
    KeyEvent { key: Key::Text(text.to_owned()), mods: Mods::default() }
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
        assert!(spans.iter().all(|s| src.is_char_boundary(s.start as usize) && src.is_char_boundary(s.end as usize)));
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
    fn multi_cursor_inserts_the_same_text_at_each_caret() {
        let mut editor = SqlEditor { edit: TextEdit::new("ab"), extra: Vec::new() };
        editor.edit.anchor = 0;
        editor.edit.focus = 0;
        editor.add_cursor(2);
        editor.insert_all("X");
        assert_eq!(editor.edit.text, "XabX");
        assert_eq!(editor.cursor_count(), 2);
        assert_eq!(editor.edit.focus, 1);
        assert_eq!(editor.extra_carets(), &[4]);
    }

    #[test]
    fn single_caret_key_is_left_to_the_field() {
        let mut editor = SqlEditor::new();
        assert!(!on_editor_key(&mut editor, &text_key("A")));
    }

    #[test]
    fn extra_caret_key_inserts_everywhere() {
        let mut editor = SqlEditor { edit: TextEdit::new("ab"), extra: vec![2] };
        editor.edit.anchor = 0;
        editor.edit.focus = 0;
        assert!(on_editor_key(&mut editor, &text_key("中")));
        assert_eq!(editor.edit.text, "中ab中");
    }
}
