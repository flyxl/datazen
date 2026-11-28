//! Byte-indexed dialect rules. Only final spans become source slices; a
//! non-ASCII byte never becomes the end of an identifier or quoted token.

use crate::{Dialect, Error, Limits};

use super::{decimal, is_alphabetic, is_ws, keywords, letter_or_mark, operators, skip_ws, TokenKind};

#[derive(Clone, Copy)]
pub(super) struct Profile {
    pub(super) dialect: Dialect,
    pub(super) nested_comments: bool,
    pub(super) mysql_family: bool,
}

impl Profile {
    pub(super) fn token(&self, text: &str, limits: &Limits) -> Result<(usize, TokenKind), Error> {
        use TokenKind::*;
        let first = *text.as_bytes().first().ok_or(Error::Lexical)?;
        if text.starts_with("/*") {
            return block_comment(text, self.nested_comments, limits).map(|len| (len, BlockComment));
        }
        if text.starts_with("--") || (self.mysql_family && first == b'#') {
            return line_comment(text).map(|len| (len, LineComment));
        }
        if let Some((quote, start)) = self.ident_quote(text) {
            return quote_end(text, start, quote, limits).map(|len| (len, QuotedIdentifier));
        }
        if let Some(len) = number(text, self.dialect == Dialect::PostgreSql) {
            return Ok((len, Number));
        }
        if self.dialect == Dialect::PostgreSql {
            if let Some(len) = postgres_operator(text) {
                return Ok((len, Operator));
            }
        }
        if let Some((len, kind)) = keywords::keyword(text, self.dialect) {
            return Ok((len, Keyword(kind)));
        }
        if let Some(len) = self.placeholder(text, limits)? {
            return Ok((len, Placeholder));
        }
        if self.mysql_family && first == b'@' {
            return mysql_variable(text, limits).map(|len| (len, Variable));
        }
        if self.dialect == Dialect::PostgreSql && first == b'$' {
            if let Some(len) = dollar_quote(text, limits)? {
                return Ok((len, String));
            }
        }
        if let Some((quote, start)) = self.string_quote(text) {
            return quote_end(text, start, quote, limits).map(|len| (len, String));
        }
        let len = self.ident_len(text);
        if len > 0 {
            return Ok((len, Word));
        }

        if matches!(first, b'(' | b')' | b',' | b';')
            || (self.dialect == Dialect::PostgreSql && matches!(first, b'[' | b']'))
        {
            return Ok((1, Punctuation));
        }
        if let Some(op) = operators::operators(self.dialect).iter().find(|&&op| text.starts_with(op)) {
            return Ok((op.len(), Operator));
        }
        if self.dialect == Dialect::TransactSql && text.starts_with("..") {
            return Ok((2, Operator));
        }
        if matches!(first, b'.' | b'*') {
            return Ok((1, Operator));
        }
        Err(Error::Lexical)
    }

    fn ident_len(self, text: &str) -> usize {
        let mut it = text.char_indices();
        let Some((_, first)) = it.next() else { return 0; };
        let first_ok = letter_or_mark(first)
            || (self.mysql_family && (first == '$' || decimal(first)))
            || (self.dialect == Dialect::TransactSql && matches!(first, '#' | '@'));
        if !first_ok {
            return 0;
        }
        it.find_map(|(at, c)| {
            let ok = letter_or_mark(c)
                || decimal(c)
                || (matches!(self.dialect, Dialect::PostgreSql | Dialect::MySql | Dialect::MariaDb)
                    && c == '$')
                || (self.dialect == Dialect::TransactSql && matches!(c, '#' | '@' | '$'));
            (!ok).then_some(at)
        }).unwrap_or(text.len())
    }

    fn ident_quote(self, text: &str) -> Option<(Quote, usize)> {
        use Dialect::*;
        match self.dialect {
            Sql | Sqlite => open_quote(text, 0, b'"', Quote::double(b'"'))
                .or_else(|| open_quote(text, 0, b'`', Quote::double(b'`')))
                .or_else(|| {
                    (self.dialect == Sqlite).then_some(())?;
                    open_quote(text, 0, b'[', Quote::double(b']'))
                }),
            PostgreSql => {
                let start = ascii_prefixed(text, "U&", b'"').unwrap_or(0);
                open_quote(text, start, b'"', Quote::double(b'"'))
            }
            MySql | MariaDb => open_quote(text, 0, b'`', Quote::double(b'`')),
            TransactSql => open_quote(text, 0, b'"', Quote::double(b'"'))
                .or_else(|| open_quote(text, 0, b'[', Quote::double(b']'))),
        }
    }

    fn string_quote(self, text: &str) -> Option<(Quote, usize)> {
        use Dialect::*;
        let normal = Quote::double(b'\'');
        let escaped = Quote::escaped(b'\'');
        let raw = Quote::raw(b'\'');
        let single = |start, quote| open_quote(text, start, b'\'', quote);
        let prefix = |p| ascii_prefixed(text, p, b'\'');
        match self.dialect {
            Sql => single(prefix("N").or_else(|| prefix("U&")).unwrap_or(0), escaped)
                .or_else(|| single(prefix("X")?, raw)),
            Sqlite => single(0, normal).or_else(|| single(prefix("X")?, raw)),
            PostgreSql => single(prefix("U&").unwrap_or(0), normal)
                .or_else(|| single(prefix("E")?, escaped))
                .or_else(|| single(prefix("B").or_else(|| prefix("X"))?, raw)),
            MySql | MariaDb => {
                let start = if self.dialect == MySql { prefix("N").unwrap_or(0) } else { 0 };
                open_quote(text, 0, b'"', Quote::escaped(b'"'))
                    .or_else(|| single(start, escaped))
                    .or_else(|| single(prefix("B").or_else(|| prefix("X"))?, raw))
            }
            TransactSql => single(prefix("N").unwrap_or(0), normal)
                .or_else(|| open_quote(text, 0, b'{', Quote::raw(b'}'))),
        }
    }

    fn placeholder(self, text: &str, limits: &Limits) -> Result<Option<usize>, Error> {
        use Dialect::*;
        let first = *text.as_bytes().first().ok_or(Error::Lexical)?;
        match (self.dialect, first) {
            (Sqlite, b'?') => Ok(Some(1 + ascii_run(&text.as_bytes()[1..], |b| b.is_ascii_digit()))),
            (Sql | MySql | MariaDb, b'?') => Ok(Some(1)),
            (PostgreSql, b'$') => {
                let count = ascii_run(&text.as_bytes()[1..], |b| b.is_ascii_digit());
                // Dollar tags also allow digits. The quote rule follows the
                // default parameter rule, so a numeric prefix wins as legacy.
                Ok((count > 0).then_some(1 + count))
            }
            (Sqlite, b':' | b'@') | (TransactSql, b'@') => {
                let after = &text[1..];
                let len = self.ident_len(after);
                if len > 0 {
                    return Ok(Some(1 + len));
                }
                if self.dialect == TransactSql {
                    if let Some((quote, start)) = self.ident_quote(after) {
                        return quote_end(after, start, quote, limits).map(|len| Some(1 + len));
                    }
                }
                Ok(None)
            }
            (Sqlite, b'$') => Ok(sqlite_dollar(text)),
            _ => Ok(None),
        }
    }
}

struct Quote {
    closing: u8,
    doubled: bool,
    backslash: bool,
}

impl Quote {
    const fn raw(closing: u8) -> Self {
        Self { closing, doubled: false, backslash: false }
    }
    const fn double(closing: u8) -> Self {
        Self { closing, doubled: true, backslash: false }
    }
    const fn escaped(closing: u8) -> Self {
        Self { closing, doubled: true, backslash: true }
    }
}

fn open_quote(text: &str, start: usize, open: u8, quote: Quote) -> Option<(Quote, usize)> {
    (text.as_bytes().get(start) == Some(&open)).then_some((quote, start))
}

fn ascii_prefixed(text: &str, prefix: &str, open: u8) -> Option<usize> {
    (text.as_bytes().get(..prefix.len())?.eq_ignore_ascii_case(prefix.as_bytes())
        && text.as_bytes().get(prefix.len()) == Some(&open)).then_some(prefix.len())
}

fn quote_end(text: &str, start: usize, quote: Quote, limits: &Limits) -> Result<usize, Error> {
    let bytes = text.as_bytes();
    let mut at = start + 1;
    let mut checkpoint = at + 4096;
    while let Some(&current) = bytes.get(at) {
        if at >= checkpoint {
            limits.check()?;
            checkpoint = at + 4096;
        }
        if current == quote.closing {
            if quote.doubled && bytes.get(at + 1) == Some(&quote.closing) {
                at += 2;
                continue;
            }
            return Ok(at + 1);
        }
        if quote.backslash && current == b'\\' {
            let escaped = text.get(at + 1..).and_then(|s| s.chars().next()).ok_or(Error::Lexical)?;
            if matches!(escaped, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                return Err(Error::Lexical);
            }
            at += 1 + escaped.len_utf8();
        } else {
            at += 1;
        }
    }
    Err(Error::Lexical)
}

fn line_comment(text: &str) -> Result<usize, Error> {
    for (at, c) in text.char_indices() {
        match c {
            '\n' | '\r' => return Ok(at),
            // ECMAScript dot excludes these, but the suffix lookahead in
            // the reference only recognizes CR and LF.
            '\u{2028}' | '\u{2029}' => return Err(Error::Lexical),
            _ => {}
        }
    }
    Ok(text.len())
}

fn block_comment(text: &str, nested: bool, limits: &Limits) -> Result<usize, Error> {
    let body = &text[2..];
    if let Some(i) = body.find("*/") {
        if body[..i].trim_matches(' ') == "sql-formatter-disable" {
            // Do not tokenize or rewrite the region behind this marker.
            return Err(Error::Unsupported);
        }
        if !nested {
            return Ok(i + 4);
        }
    } else {
        return Err(Error::Lexical);
    }
    let mut depth = 1usize;
    let mut cursor = 2;
    let mut checkpoint = cursor + 4096;
    let bytes = text.as_bytes();
    while cursor + 1 < bytes.len() {
        if cursor >= checkpoint {
            limits.check()?;
            checkpoint = cursor + 4096;
        }
        match (bytes[cursor], bytes[cursor + 1]) {
            (b'/', b'*') => {
                depth += 1;
                cursor += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                cursor += 2;
                if depth == 0 {
                    return Ok(cursor);
                }
            }
            _ => cursor += 1,
        }
    }
    Err(Error::Lexical)
}

fn ascii_run(bytes: &[u8], matches: impl Fn(u8) -> bool) -> usize {
    bytes.iter().position(|&b| !matches(b)).unwrap_or(bytes.len())
}

fn tcl_ident(text: &str) -> usize {
    let Some(&first) = text.as_bytes().first() else { return 0; };
    if !first.is_ascii_alphabetic() && first != b'_' {
        return 0;
    }
    ascii_run(text.as_bytes(), |b| b.is_ascii_alphanumeric() || b == b'_')
}

fn sqlite_dollar(text: &str) -> Option<usize> {
    let first = tcl_ident(&text[1..]);
    if first == 0 {
        return None;
    }
    let mut end = 1 + first;
    while let Some(rest) = text[end..].strip_prefix("::") {
        let part = tcl_ident(rest);
        if part == 0 {
            break;
        }
        end += 2 + part;
    }
    if let Some(tail) = text[end..].strip_prefix('(') {
        if let Some(close) = tail.find(')') {
            end += 2 + close;
        }
    }
    Some(end)
}

fn dollar_quote(text: &str, limits: &Limits) -> Result<Option<usize>, Error> {
    let word = ascii_run(&text.as_bytes()[1..], |b| b.is_ascii_alphanumeric() || b == b'_');
    let end = word + 2;
    if text.as_bytes().get(end - 1) != Some(&b'$') {
        return Ok(None);
    }
    limits.check()?;
    let needle = &text[..end];
    let close = text[end..].find(needle).ok_or(Error::Lexical)?;
    Ok(Some(end + close + needle.len()))
}

fn mysql_variable(text: &str, limits: &Limits) -> Result<usize, Error> {
    let bytes = text.as_bytes();
    let prefix = if bytes.get(1) == Some(&b'@') { 2 } else { 1 };
    let plain = ascii_run(&bytes[prefix..], |b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$'));
    if plain > 0 {
        return Ok(prefix + plain);
    }
    if prefix == 1 {
        match bytes.get(1) {
            Some(b'"') => return quote_end(text, 1, Quote::escaped(b'"'), limits),
            Some(b'\'') => return quote_end(text, 1, Quote::escaped(b'\''), limits),
            Some(b'`') => return quote_end(text, 1, Quote::double(b'`'), limits),
            _ => {}
        }
    }
    Err(Error::Lexical)
}

fn postgres_operator(text: &str) -> Option<usize> {
    let start = "OPERATOR".len();
    if !text.get(..start)?.eq_ignore_ascii_case("OPERATOR") {
        return None;
    }
    let rest = text[start..].trim_start_matches(' ').strip_prefix('(')?;
    let stop = rest.find(')')?;
    (stop > 0).then_some(text.len() - rest.len() + stop + 1)
}

fn number(text: &str, underscores: bool) -> Option<usize> {
    let bytes = text.as_bytes();
    let valid_end = |n: usize| {
        !text[n..].chars().next().is_some_and(|c| {
            c.is_ascii_alphanumeric() || c == '_' || is_alphabetic(c)
        })
    };
    for (prefix, radix) in [(b"0x", 16), (b"0b", 2)] {
        if bytes.starts_with(prefix) {
            let count = ascii_run(&bytes[2..], |b| {
                (if radix == 16 { b.is_ascii_hexdigit() } else { matches!(b, b'0' | b'1') })
                    || (underscores && b == b'_')
            });
            if count > 0 && valid_end(2 + count) {
                return Some(2 + count);
            }
        }
    }
    let digit = |b: u8| b.is_ascii_digit() || (underscores && b == b'_');
    let start = if bytes.first() == Some(&b'-') { 1 + skip_ws(&text[1..]) } else { 0 };
    let int = start + ascii_run(&bytes[start..], digit);
    let mut frac = int;
    if bytes.get(int) == Some(&b'.') {
        let dec = ascii_run(&bytes[int + 1..], digit);
        if dec > 0 || int > start {
            frac = int + 1 + dec;
        }
    }
    if frac == start {
        return None;
    }
    if matches!(bytes.get(frac), Some(b'e' | b'E')) {
        let signed = frac + 1 + usize::from(matches!(bytes.get(frac + 1), Some(b'+' | b'-')));
        let exp = signed + ascii_run(&bytes[signed..], digit);
        if exp > signed {
            if bytes.get(exp) == Some(&b'.') {
                let suffix = ascii_run(&bytes[exp + 1..], digit);
                if suffix > 0 && valid_end(exp + 1 + suffix) {
                    return Some(exp + 1 + suffix);
                }
            }
            if valid_end(exp) {
                return Some(exp);
            }
        }
    }
    if valid_end(frac) {
        Some(frac)
    } else if int > start && int < frac && valid_end(int) {
        // A dot can be a property access operator; the decimal rule may
        // backtrack to the valid integer prefix at that boundary.
        Some(int)
    } else {
        None
    }
}
