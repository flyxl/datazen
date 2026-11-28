//! A bounded SQL formatter, written in Rust. The original desktop formatter
//! (sql-formatter 15.8.2) is a behavioral reference, not a runtime dependency.

#![forbid(unsafe_code)]

mod layout;
mod lexer;

use std::{fmt, time::{Duration, Instant}};

pub const MAX_INPUT_BYTES: usize = 256 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_LINES_BETWEEN_QUERIES: u8 = 16;
const FORMAT_BUDGET: Duration = Duration::from_millis(700);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dialect { Sql, Sqlite, PostgreSql, MySql, MariaDb, TransactSql }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeywordCase { Preserve, Upper, Lower }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Indent { Spaces(u8), Tabs }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalOperatorNewline { Before, After }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Options {
    pub dialect: Dialect,
    pub keyword_case: KeywordCase,
    pub indent: Indent,
    pub logical_operator_newline: LogicalOperatorNewline,
    pub lines_between_queries: u8,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            dialect: Dialect::Sql,
            keyword_case: KeywordCase::Upper,
            indent: Indent::Spaces(2),
            logical_operator_newline: LogicalOperatorNewline::Before,
            lines_between_queries: 1,
        }
    }
}

/// No error contains original SQL, offsets with source, or parser internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error { Unsupported, Lexical, Deadline, InputLimit, OutputLimit, InvalidOptions }

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unsupported => "unsupported SQL format syntax or dialect",
            Self::Lexical => "invalid SQL token",
            Self::Deadline => "SQL formatting timed out",
            Self::InputLimit => "SQL format input limit exceeded",
            Self::OutputLimit => "SQL format output limit exceeded",
            Self::InvalidOptions => "invalid SQL format options",
        })
    }
}

impl std::error::Error for Error {}

/// Current explicitly implemented dialects. Lexical recognition alone is
/// not a claim that complete formatting is available.
pub const fn supports_dialect(dialect: Dialect) -> bool {
    matches!(dialect, Dialect::Sql | Dialect::Sqlite)
}

pub(crate) struct Limits { deadline: Instant }

impl Default for Limits {
    fn default() -> Self { Self { deadline: Instant::now() + FORMAT_BUDGET } }
}

impl Limits {
    pub(crate) fn check(&self) -> Result<(), Error> { check_deadline(self.deadline) }
}

pub(crate) fn check_deadline(deadline: Instant) -> Result<(), Error> {
    if Instant::now() >= deadline { Err(Error::Deadline) } else { Ok(()) }
}

fn trim_like_reference(sql: &str) -> &str {
    sql.trim_matches(|c: char| matches!(c as u32,
        0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a
        | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff))
}

/// Format an owned editor selection or whole draft, without contacting a
/// database or any external process. Successful no-op output stays exact.
pub fn format(sql: &str, options: &Options) -> Result<String, Error> {
    format_with_limits(sql, options, MAX_OUTPUT_BYTES, Instant::now() + FORMAT_BUDGET)
}

fn format_with_limits(sql: &str, options: &Options, output_limit: usize, deadline: Instant) -> Result<String, Error> {
    if sql.len() > MAX_INPUT_BYTES { return Err(Error::InputLimit); }
    if !matches!(options.indent, Indent::Spaces(2 | 4) | Indent::Tabs)
        || options.lines_between_queries > MAX_LINES_BETWEEN_QUERIES
    {
        return Err(Error::InvalidOptions);
    }
    check_deadline(deadline)?;
    let text = trim_like_reference(sql);
    if text.is_empty() {
        return (sql.len() <= output_limit).then(|| sql.to_owned()).ok_or(Error::OutputLimit);
    }
    if !supports_dialect(options.dialect) {
        return Err(Error::Unsupported);
    }
    // The token and layout stages both work from trusted typed options and
    // bounded byte slices. No interpreter or generated runtime assets.
    let tokens = lexer::tokenize(text, options.dialect, &Limits { deadline })?;
    let output = layout::format_tokens(&tokens, options, output_limit, deadline)?;
    check_deadline(deadline)?;
    (output.len() <= output_limit).then_some(output).ok_or(Error::OutputLimit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_and_limits_are_exact() {
        let options = Options::default();
        assert_eq!(format("\u{feff} \r\n\t", &options).as_deref(), Ok("\u{feff} \r\n\t"));
        assert_eq!(trim_like_reference("\u{0085}"), "\u{0085}");
        assert_eq!(format(&"x".repeat(MAX_INPUT_BYTES + 1), &options), Err(Error::InputLimit));
        assert_eq!(format("select 1;", &Options { indent: Indent::Spaces(5), ..options }), Err(Error::InvalidOptions));
        assert_eq!(format_with_limits("select 1;", &options, MAX_OUTPUT_BYTES, Instant::now()), Err(Error::Deadline));
        assert_eq!(format_with_limits("   ", &options, 2, Instant::now() + FORMAT_BUDGET), Err(Error::OutputLimit));
    }
}
