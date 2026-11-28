//! The statement ranges/cursor lookup from the React extension point,
//! projected to the fields used by formatting. Not an execution splitter.
use std::ops::Range;

const LF: u16 = 10;
const HASH: u16 = b'#' as u16;
const DASH: u16 = b'-' as u16;
const SLASH: u16 = b'/' as u16;
const STAR: u16 = b'*' as u16;
const SINGLE: u16 = b'\'' as u16;
const DOUBLE: u16 = b'"' as u16;
const BACKTICK: u16 = b'`' as u16;
const LEFT_BRACKET: u16 = b'[' as u16;
const RIGHT_BRACKET: u16 = b']' as u16;
const DOLLAR: u16 = b'$' as u16;
const SEMICOLON: u16 = b';' as u16;
const LEFT_PAREN: u16 = b'(' as u16;
const RIGHT_PAREN: u16 = b')' as u16;
const COMMA: u16 = b',' as u16;

pub(super) fn map_cursor_by_statement(original: &str, formatted: &str, cursor: usize) -> usize {
    // The real editor exposes UTF-8 byte offsets, while CodeMirror and its
    // scanner use UTF-16 units. Keep the React ordinal and fallback in that
    // coordinate space, then land on a valid Native scalar boundary.
    let cursor = original.floor_char_boundary(cursor.min(original.len()));
    let cursor = original[..cursor].encode_utf16().count();
    let source: Vec<_> = original.encode_utf16().collect();
    let index = statement_at_cursor(&source, cursor);
    let anchor = index.and_then(|index| {
        let target: Vec<_> = formatted.encode_utf16().collect();
        let scan = scan_boundaries(&target, None);
        // Delimiter-only slots still count on the destination side, exactly
        // as buildStatementRanges().find(r => r.index === before.index).
        statement_ranges(&target, &scan.ends).get(index).map(|statement| statement.content.start)
    });
    byte_at_utf16_floor(formatted, anchor.unwrap_or(cursor))
}

fn byte_at_utf16_floor(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        units += ch.len_utf16();
        if offset < units {
            return byte;
        }
    }
    text.len()
}

struct Statement {
    physical: Range<usize>,
    content: Range<usize>,
}

fn layout_whitespace(unit: u16) -> bool {
    matches!(unit, 9 | 10 | 12 | 13 | 32)
}

fn regex_whitespace(unit: u16) -> bool {
    // ECMAScript /\s/ is broader than scanner/layout whitespace.
    matches!(unit, 9..=13 | 32 | 0x00a0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029
        | 0x202f | 0x205f | 0x3000 | 0xfeff)
}

fn ascii_word(unit: u16) -> bool {
    matches!(unit, 48..=57 | 65..=90 | 95 | 97..=122)
}

fn word_at(source: &[u16], start: usize, word: &[u8]) -> Option<usize> {
    let end = start + word.len();
    let matching = source.get(start..end)?.iter().zip(word).all(|(&unit, &expected)| {
        u8::try_from(unit).is_ok_and(|byte| byte.eq_ignore_ascii_case(&expected))
    });
    (matching && !source.get(end).copied().is_some_and(ascii_word)).then_some(end)
}

fn space_after(source: &[u16], start: usize) -> Option<usize> {
    let width = source[start..].iter().take_while(|&&unit| regex_whitespace(unit)).count();
    (width > 0).then_some(start + width)
}

fn degraded(source: &[u16]) -> bool {
    // Raw /\bDELIMITER\b|\bCREATE\s+(?:OR\s+REPLACE\s+)?(?:PROCEDURE|FUNCTION|TRIGGER)\b/i.
    // As in React, occurrences in comments or quoted text still trigger it.
    source.iter().enumerate().any(|(i, &unit)| {
        if !matches!(unit, 67 | 68 | 99 | 100) || (i > 0 && ascii_word(source[i - 1])) {
            return false;
        }
        if word_at(source, i, b"DELIMITER").is_some() {
            return true;
        }
        let Some(routine) = word_at(source, i, b"CREATE").and_then(|end| space_after(source, end)) else {
            return false;
        };
        let routine = word_at(source, routine, b"OR")
            .and_then(|end| space_after(source, end))
            .and_then(|start| word_at(source, start, b"REPLACE"))
            .and_then(|end| space_after(source, end))
            .unwrap_or(routine);
        [b"PROCEDURE".as_slice(), b"FUNCTION", b"TRIGGER"]
            .iter().any(|keyword| word_at(source, routine, keyword).is_some())
    })
}

fn trim_content(source: &[u16], start: usize, end: usize) -> Range<usize> {
    let from = start + source[start..end].iter().take_while(|&&unit| layout_whitespace(unit)).count();
    let mut to = end;
    while to > from && layout_whitespace(source[to - 1]) {
        to -= 1;
    }
    if to > from && source[to - 1] == SEMICOLON {
        to -= 1;
        while to > from && layout_whitespace(source[to - 1]) {
            to -= 1;
        }
    }
    from..to
}

fn statement_ranges(source: &[u16], ends: &[usize]) -> Vec<Statement> {
    if degraded(source) {
        return vec![Statement {
            physical: 0..source.len(),
            content: trim_content(source, 0, source.len()),
        }];
    }
    let mut ranges = Vec::new();
    let mut from = 0;
    for to in ends.iter().copied().chain(std::iter::once(source.len())) {
        if from < to && source[from..to].iter().any(|&unit| !layout_whitespace(unit)) {
            ranges.push(Statement { physical: from..to, content: trim_content(source, from, to) });
        }
        from = to;
    }
    ranges
}

fn statement_at_cursor(source: &[u16], cursor: usize) -> Option<usize> {
    let scan = scan_boundaries(source, Some(cursor));
    let ranges = statement_ranges(source, &scan.ends);
    let mut valid = ranges.iter().enumerate().filter(|(_, range)| !range.content.is_empty()).peekable();
    let first = valid.peek()?.0;
    let mut left = None;
    for (index, statement) in valid {
        if cursor < statement.physical.start {
            break;
        }
        if cursor <= statement.physical.end {
            if cursor < statement.content.start {
                // Only the prior physical ordinal qualifies. Do not skip over
                // an intervening separator-only/empty-content statement.
                return left.filter(|&prev| prev + 1 == index).or(Some(index));
            }
            return (!scan.on_comment || has_executable_code(&source[statement.physical.clone()]))
                .then_some(index);
        }
        left = Some(index);
    }
    left.or(Some(first))
}

fn next_line(source: &[u16], from: usize) -> usize {
    source[from..].iter().position(|&unit| unit == LF).map_or(source.len(), |n| from + n)
}

fn end_block(source: &[u16], body_start: usize) -> Option<usize> {
    source[body_start..].windows(2).position(|pair| pair == [STAR, SLASH])
        .map(|offset| body_start + offset + 2)
}

fn end_quote(source: &[u16], mut pos: usize, close: u16, doubled_with: Option<u16>) -> usize {
    while pos < source.len() {
        if source[pos] == close {
            if doubled_with.is_some_and(|second| source.get(pos + 1) == Some(&second)) {
                pos += 2;
                continue;
            }
            return pos + 1;
        }
        pos += 1;
    }
    pos
}

fn end_dollar(source: &[u16], from: usize) -> Option<usize> {
    let tag_end = from + 1 + source[from + 1..].iter().take_while(|&&unit| ascii_word(unit)).count();
    if source.get(tag_end) != Some(&DOLLAR) {
        return None;
    }
    let tag = &source[from..=tag_end];
    source[tag_end + 1..].windows(tag.len()).position(|window| window == tag)
        .map(|found| tag_end + 1 + found + tag.len())
}

fn comment_end(source: &[u16], i: usize, keep_lexical_tail: bool) -> Option<usize> {
    let next = source.get(i + 1).copied();
    match source[i] {
        HASH => Some(next_line(source, i + 1)),
        DASH if next == Some(DASH) => Some(next_line(source, i + 2)),
        SLASH if next == Some(STAR) => {
            let body = i + 2;
            // scanner.ts's unterminated block token excludes the last UTF-16
            // unit (if any). Its separate executable-content probe does not.
            let unclosed = if keep_lexical_tail { body.max(source.len() - 1) } else { source.len() };
            Some(end_block(source, body).unwrap_or(unclosed))
        }
        _ => None,
    }
}

fn other_break(source: &[u16], i: usize) -> bool {
    match source[i] {
        HASH | SINGLE | DOUBLE | BACKTICK | LEFT_BRACKET | SEMICOLON | LEFT_PAREN |
        RIGHT_PAREN | COMMA => true,
        DASH => source.get(i + 1) == Some(&DASH),
        SLASH => source.get(i + 1) == Some(&STAR),
        unit => layout_whitespace(unit),
    }
}

#[derive(Default)]
struct Scan {
    ends: Vec<usize>,
    on_comment: bool,
}

fn scan_boundaries(source: &[u16], cursor: Option<usize>) -> Scan {
    let mut scan = Scan::default();
    let mut depth = 0usize;
    let mut i = 0;
    while i < source.len() {
        if let Some(end) = comment_end(source, i, true) {
            scan.on_comment |= cursor.is_some_and(|at| (i..end).contains(&at));
            i = end;
            continue;
        }
        match source[i] {
            quote @ (SINGLE | DOUBLE) => i = end_quote(source, i + 1, quote, Some(quote)),
            BACKTICK => i = end_quote(source, i + 1, BACKTICK, None),
            LEFT_BRACKET => i = end_quote(source, i + 1, RIGHT_BRACKET, Some(RIGHT_BRACKET)),
            SEMICOLON if depth == 0 => {
                i += 1;
                scan.ends.push(i);
            }
            LEFT_PAREN => {
                depth += 1;
                i += 1;
            }
            RIGHT_PAREN => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            unit if unit == COMMA || unit == SEMICOLON || layout_whitespace(unit) => i += 1,
            unit => {
                if unit == DOLLAR {
                    if let Some(end) = end_dollar(source, i) {
                        i = end;
                        continue;
                    }
                }
                // '$' does not break an ordinary run in scanner.ts, and an
                // unclosed dollar tag itself is likewise an ordinary run.
                i += 1;
                while i < source.len() && !other_break(source, i) {
                    i += 1;
                }
            }
        }
    }
    scan
}

fn has_executable_code(source: &[u16]) -> bool {
    let mut i = 0;
    while i < source.len() {
        if let Some(end) = comment_end(source, i, false) {
            i = end;
            continue;
        }
        match source[i] {
            quote @ (SINGLE | DOUBLE) => i = end_quote(source, i + 1, quote, Some(quote)),
            BACKTICK => i = end_quote(source, i + 1, BACKTICK, None),
            // Deliberately mirrors this separate TS fallback, *not* the
            // boundary scanner's ']' rule. This is used only inside comments.
            LEFT_BRACKET => i = end_quote(source, i + 1, LEFT_BRACKET, Some(RIGHT_BRACKET)),
            unit if layout_whitespace(unit) => i += 1,
            _ => return true,
        }
    }
    false
}
