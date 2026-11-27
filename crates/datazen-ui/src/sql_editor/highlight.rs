use datazen_ui_api::SqlSyntaxTheme;
use rune::Span;

use super::syntax_theme::SyntaxPalette;

use super::model::{is_keyword, KEYWORDS};

pub fn highlight_sql(src: &str, dark: bool) -> Vec<Span> {
    highlight_with_palette(src, SyntaxPalette::live(SqlSyntaxTheme::DEFAULT, dark))
}

pub(crate) fn highlight_with_palette(src: &str, palette: SyntaxPalette) -> Vec<Span> {
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
            spans.push(Span::new(start, i).color(palette.tokens.comment).italic());
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            spans.push(Span::new(start, i).color(palette.tokens.comment).italic());
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
            spans.push(Span::new(start, i).color(palette.tokens.string));
            continue;
        }
        if bytes[i].is_ascii_digit() {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            spans.push(Span::new(start, i).color(palette.tokens.number));
            continue;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if is_keyword(&src[start..i]) {
                spans.push(Span::new(start, i).color(palette.tokens.keyword).bold());
            }
            continue;
        }
        let color = match bytes[i] {
            b'=' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'!' | b'|' | b'&'
                => palette.operator,
            b'.' | b',' | b';' | b'(' | b')' | b'[' | b']' => palette.punctuation,
            _ => None,
        };
        if let Some(color) = color {
            spans.push(Span::new(i, i + 1).color(color));
        }
        i += 1;
    }
    spans
}

// keep KEYWORDS referenced to avoid unused warning pattern in some builds? not needed.
#[allow(dead_code)]
fn _keywords_len() -> usize { KEYWORDS.len() }
