use rune::Span;

use crate::ui::palette;

use super::model::{is_keyword, KEYWORDS};

pub fn highlight_sql(src: &str, dark: bool) -> Vec<Span> {
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
            spans.push(Span::new(start, i).color(comment_color(dark)).italic());
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            spans.push(Span::new(start, i).color(comment_color(dark)).italic());
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
            spans.push(Span::new(start, i).color(string_color(dark)));
            continue;
        }
        if bytes[i].is_ascii_digit() {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            spans.push(Span::new(start, i).color(number_color(dark)));
            continue;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if is_keyword(&src[start..i]) {
                spans.push(Span::new(start, i).color(keyword_color(dark)).bold());
            }
            continue;
        }
        i += 1;
    }
    spans
}

pub(super) fn keyword_color(dark: bool) -> rune::Color {
    palette::code_palette(dark).keyword
}
pub(super) fn string_color(dark: bool) -> rune::Color {
    palette::code_palette(dark).string
}
pub(super) fn comment_color(dark: bool) -> rune::Color {
    palette::code_palette(dark).comment
}
pub(super) fn number_color(dark: bool) -> rune::Color {
    palette::code_palette(dark).number
}

// keep KEYWORDS referenced to avoid unused warning pattern in some builds? not needed.
#[allow(dead_code)]
fn _keywords_len() -> usize { KEYWORDS.len() }
