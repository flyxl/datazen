//! Borrowed, bounded tokenizer for the Native formatter.
//!
//! Rule ordering, six dialect tables and special token names follow the
//! pinned sql-formatter 15.8.2 reference; text remains the original UTF-8
//! slice, including whitespace inside phrases. Formatting never executes SQL.

use crate::{Dialect, Error, Limits, MAX_INPUT_BYTES};

mod keywords;
mod operators;
mod scan;
mod unicode;

const MAX_TOKENS: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeywordKind {
    Keyword,
    DataType,
    ParameterizedDataType,
    Function,
    Select,
    Clause,
    SetOperation,
    Join,
    LogicalOperator,
    Case,
    Between,
    Limit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Word,
    Keyword(KeywordKind),
    Number,
    String,
    QuotedIdentifier,
    Placeholder,
    Variable,
    Operator,
    Punctuation,
    LineComment,
    BlockComment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Token<'s> {
    pub(crate) text: &'s str,
    pub(crate) offset: usize,
    pub(crate) whitespace_before: &'s str,
    pub(crate) kind: TokenKind,
}

pub(crate) fn is_ws(c: char) -> bool {
    matches!(
        c as u32,
        0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a
            | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff
    )
}

fn skip_ws(text: &str) -> usize {
    text.find(|c: char| !is_ws(c)).unwrap_or(text.len())
}

fn contains(ranges: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    let i = ranges.partition_point(|&(_, end)| end < c);
    ranges.get(i).is_some_and(|&(start, _)| start <= c)
}

fn letter_or_mark(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphabetic() || c == '_'
    } else {
        contains(unicode::LETTER_OR_MARK, c)
    }
}

fn is_alphabetic(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphabetic()
    } else {
        contains(unicode::ALPHABETIC, c)
    }
}

fn decimal(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_digit()
    } else {
        contains(unicode::DECIMAL, c)
    }
}

fn dialect_profile(dialect: Dialect) -> scan::Profile {
    use Dialect::*;
    scan::Profile {
        dialect,
        nested_comments: matches!(dialect, PostgreSql | TransactSql),
        mysql_family: matches!(dialect, MySql | MariaDb),
    }
}

pub(crate) fn tokenize<'s>(
    sql: &'s str,
    dialect: Dialect,
    limits: &Limits,
) -> Result<Vec<Token<'s>>, Error> {
    limits.check()?;
    if sql.len() > MAX_INPUT_BYTES {
        return Err(Error::InputLimit);
    }
    let profile = dialect_profile(dialect);
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < sql.len() {
        limits.check()?;
        let start = at + skip_ws(&sql[at..]);
        if start == sql.len() {
            break;
        }
        if tokens.len() >= MAX_TOKENS {
            return Err(Error::InputLimit);
        }
        let (bytes, kind) = profile.token(&sql[start..], limits)?;
        let end = start + bytes;
        let text = sql.get(start..end).filter(|text| !text.is_empty()).ok_or(Error::Lexical)?;
        tokens.try_reserve(1).map_err(|_| Error::InputLimit)?;
        tokens.push(Token { text, offset: start, whitespace_before: &sql[at..start], kind });
        at = end;
    }
    disambiguate(&mut tokens, profile, limits)?;
    Ok(tokens)
}

fn is_comment(token: &Token<'_>) -> bool {
    matches!(token.kind, TokenKind::LineComment | TokenKind::BlockComment)
}

fn is_property(text: &str, dialect: Dialect) -> bool {
    text == "." || (dialect == Dialect::TransactSql && text == "..")
}

fn disambiguate(tokens: &mut [Token<'_>], p: scan::Profile, limits: &Limits) -> Result<(), Error> {
    use KeywordKind as K;
    use TokenKind as T;
    // MySQL/MariaDB postprocess precedes the generic qualification pass.
    if p.mysql_family {
        for i in 0..tokens.len() {
            if i % 256 == 0 {
                limits.check()?;
            }
            if tokens[i].kind != T::Keyword(K::Clause) {
                continue;
            }
            let is_set_type = tokens[i].text.eq_ignore_ascii_case("SET")
                && tokens.get(i + 1).is_some_and(|t| t.text == "(");
            let is_values_fn = tokens[i].text.eq_ignore_ascii_case("VALUES")
                && i.checked_sub(1).and_then(|i| tokens.get(i)).is_some_and(|t| t.text == "=");
            if is_set_type || is_values_fn {
                tokens[i].kind = T::Keyword(K::Function);
            }
        }
    }
    let mut prev: Option<Token<'_>> = None;
    for i in 0..tokens.len() {
        if i % 256 == 0 {
            limits.check()?;
        }
        if is_comment(&tokens[i]) {
            continue;
        }
        if let T::Keyword(k) = tokens[i].kind {
            let next = tokens[i + 1..].iter().find(|t| !is_comment(t));
            let touches_property = |t: Option<&Token<'_>>| {
                t.is_some_and(|t| t.kind == T::Operator && is_property(t.text, p.dialect))
            };
            let has_parens = next.is_some_and(|t| t.kind == T::Punctuation && t.text == "(");
            tokens[i].kind = if touches_property(prev.as_ref())
                || touches_property(next)
                || (k == K::Function && !has_parens)
            {
                T::Word
            } else if k == K::DataType && has_parens && !tokens[i].text.contains(is_ws) {
                T::Keyword(K::ParameterizedDataType)
            } else {
                T::Keyword(k)
            };
        }
        prev = Some(tokens[i]);
    }
    limits.check()
}

#[cfg(test)]
mod tests;
