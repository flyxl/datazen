//! Shared SQL-aware template rendering for plugin and Workflow steps.
//!
//! The renderer is deliberately independent of the AI and Workflow modules so
//! both callers exercise one lexer. A caller supplies a typed resolver; the
//! renderer never performs a raw string replacement.

use datazen_driver_api::SqlLiteralDialect;
use serde_json::{Map, Value};

/// Whether an input key denotes a SQL template (the same rule for plugins and
/// workflow commands).
pub fn is_sql_template_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower == "sql" || lower.ends_with("_sql")
}

/// How a SQL template treats backslashes inside single-quoted strings.
///
/// The host chooses this from the target connection's driver. The shared
/// renderer defaults to the SQL standard, where a backslash is an ordinary
/// character; MySQL-family engines use backslash escapes instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SqlTemplateDialect {
    #[default]
    Standard,
    BackslashEscapes,
}

impl From<SqlLiteralDialect> for SqlTemplateDialect {
    fn from(dialect: SqlLiteralDialect) -> Self {
        match dialect {
            SqlLiteralDialect::MySql | SqlLiteralDialect::ClickHouse => Self::BackslashEscapes,
            SqlLiteralDialect::DuckDb
            | SqlLiteralDialect::Postgres
            | SqlLiteralDialect::SqlServer
            | SqlLiteralDialect::Sqlite => Self::Standard,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SqlLexState {
    Normal,
    Single { escape_backslash: bool },
    Double,
    Backtick,
    Bracket,
    LineComment,
    BlockComment { depth: usize },
    DollarQuote { delimiter: String },
}

/// Render a SQL template with a typed resolver.
///
/// This is the single lexer used by plugin templates and Workflow SQL steps.
/// The resolver returns the actual JSON value for a placeholder; rendering
/// never falls back to a raw string replacement.
pub fn render_sql_template(
    template: &str,
    arguments: &Map<String, Value>,
    dialect: SqlTemplateDialect,
) -> Result<String, String> {
    render_sql_template_with_resolver(template, dialect, |name| {
        arguments
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing template argument '{name}'"))
    })
}

/// Render a SQL template without ever concatenating an arbitrary value into
/// an identifier or expression.
///
/// The scanner is deliberately small but strict:
/// - placeholders inside single-quoted literals become escaped string content;
/// - placeholders outside a string must be a number/bool/NULL and token
///   delimited, otherwise the call is rejected;
/// - placeholders in double-quoted identifiers, dollar quotes and comments are
///   rejected (fail closed) rather than guessed;
/// - unterminated quotes/comments/dollar quotes and unterminated placeholders
///   are errors.
pub fn render_sql_template_with_resolver<F>(
    template: &str,
    dialect: SqlTemplateDialect,
    mut resolve: F,
) -> Result<String, String>
where
    F: FnMut(&str) -> Result<Value, String>,
{
    let mut output = String::with_capacity(template.len());
    let mut state = SqlLexState::Normal;
    let mut index = 0;

    while index < template.len() {
        let rest = &template[index..];
        if let Some(rest) = rest.strip_prefix("{{") {
            let Some(end) = rest.find("}}") else {
                return Err("unterminated template placeholder".into());
            };
            let name = &rest[..end];
            if name.is_empty() || name.contains("{{") || name.contains("}}") {
                return Err("template placeholder name is invalid".into());
            }
            let argument = resolve(name)?;
            match &state {
                SqlLexState::Normal => {
                    let literal = safe_sql_literal(&argument)?;
                    if !template_boundary_before(template, index)
                        || !template_boundary_after(template, index + 2 + end + 2)
                    {
                        return Err(format!(
                            "template argument '{name}' cannot be concatenated into a SQL token"
                        ));
                    }
                    output.push_str(&literal);
                }
                SqlLexState::Single { escape_backslash } => {
                    output.push_str(&sql_string_content(&argument, *escape_backslash)?);
                }
                SqlLexState::Double => {
                    return Err(format!(
                        "template argument '{name}' inside a double-quoted identifier is not allowed"
                    ));
                }
                SqlLexState::DollarQuote { .. } => {
                    return Err(format!(
                        "template argument '{name}' inside a dollar-quoted string is not allowed"
                    ));
                }
                SqlLexState::LineComment | SqlLexState::BlockComment { .. } => {
                    return Err(format!(
                        "template argument '{name}' inside a SQL comment is not allowed"
                    ));
                }
                SqlLexState::Backtick | SqlLexState::Bracket => {
                    return Err(format!(
                        "template argument '{name}' inside a quoted identifier is not allowed"
                    ));
                }
            }
            index += 2 + end + 2;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        let next = match &state {
            SqlLexState::Normal => {
                if rest.starts_with("--") {
                    output.push_str("--");
                    index += 2;
                    SqlLexState::LineComment
                } else if rest.starts_with("/*") {
                    output.push_str("/*");
                    index += 2;
                    SqlLexState::BlockComment { depth: 1 }
                } else if ch == '#' && dialect == SqlTemplateDialect::BackslashEscapes {
                    output.push('#');
                    index += 1;
                    SqlLexState::LineComment
                } else if let Some((delimiter, len)) = dollar_quote_open(rest) {
                    output.push_str(delimiter);
                    index += len;
                    SqlLexState::DollarQuote {
                        delimiter: delimiter.to_string(),
                    }
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    match ch {
                        '\'' => SqlLexState::Single {
                            escape_backslash: dialect == SqlTemplateDialect::BackslashEscapes
                                || has_postgres_escape_prefix(template, index - ch.len_utf8()),
                        },
                        '"' => SqlLexState::Double,
                        '`' => SqlLexState::Backtick,
                        '[' => SqlLexState::Bracket,
                        _ => SqlLexState::Normal,
                    }
                }
            }
            SqlLexState::Single { escape_backslash } => {
                if ch == '\'' {
                    if rest[1..].starts_with('\'') {
                        output.push_str("''");
                        index += 2;
                        SqlLexState::Single {
                            escape_backslash: *escape_backslash,
                        }
                    } else {
                        output.push('\'');
                        index += 1;
                        SqlLexState::Normal
                    }
                } else if ch == '\\' && *escape_backslash {
                    let mut chars = rest.chars();
                    chars.next();
                    let Some(escaped) = chars.next() else {
                        return Err("unterminated escape at end of SQL template".into());
                    };
                    // MySQL can run with NO_BACKSLASH_ESCAPES, where `\'`
                    // closes the string instead of escaping the quote. The
                    // same template would then have a different string
                    // context, so reject that ambiguous spelling rather than
                    // render a value under the wrong assumption.
                    if dialect == SqlTemplateDialect::BackslashEscapes && escaped == '\'' {
                        return Err(
                            "a backslash-escaped quote is ambiguous in a backslash-escape SQL dialect"
                                .into(),
                        );
                    }
                    // In a backslash-escape string `\{{` would consume the
                    // opening brace of a placeholder as an escaped character.
                    // Reject instead of silently leaving a half-rendered
                    // template in the SQL text.
                    if escaped == '{' && rest[1..].starts_with("{{") {
                        return Err("template placeholder cannot follow a backslash escape".into());
                    }
                    output.push('\\');
                    output.push(escaped);
                    index += 1 + escaped.len_utf8();
                    SqlLexState::Single {
                        escape_backslash: true,
                    }
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::Single {
                        escape_backslash: *escape_backslash,
                    }
                }
            }
            SqlLexState::Double => {
                if ch == '"' {
                    if rest[1..].starts_with('"') {
                        output.push_str("\"\"");
                        index += 2;
                        SqlLexState::Double
                    } else {
                        output.push('"');
                        index += 1;
                        SqlLexState::Normal
                    }
                } else if ch == '\\' && dialect == SqlTemplateDialect::BackslashEscapes {
                    let mut chars = rest.chars();
                    chars.next();
                    let Some(escaped) = chars.next() else {
                        return Err("unterminated escape at end of SQL template".into());
                    };
                    output.push('\\');
                    output.push(escaped);
                    index += 1 + escaped.len_utf8();
                    SqlLexState::Double
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::Double
                }
            }
            SqlLexState::Backtick => {
                if ch == '`' {
                    if rest[1..].starts_with('`') {
                        output.push_str("``");
                        index += 2;
                        SqlLexState::Backtick
                    } else {
                        output.push('`');
                        index += 1;
                        SqlLexState::Normal
                    }
                } else if ch == '\\' && dialect == SqlTemplateDialect::BackslashEscapes {
                    let mut chars = rest.chars();
                    chars.next();
                    let Some(escaped) = chars.next() else {
                        return Err("unterminated escape at end of SQL template".into());
                    };
                    output.push('\\');
                    output.push(escaped);
                    index += 1 + escaped.len_utf8();
                    SqlLexState::Backtick
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::Backtick
                }
            }
            SqlLexState::Bracket => {
                if ch == ']' {
                    if rest[1..].starts_with(']') {
                        output.push_str("]]");
                        index += 2;
                        SqlLexState::Bracket
                    } else {
                        output.push(']');
                        index += 1;
                        SqlLexState::Normal
                    }
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::Bracket
                }
            }
            SqlLexState::LineComment => {
                output.push(ch);
                index += ch.len_utf8();
                if ch == '\n' {
                    SqlLexState::Normal
                } else {
                    SqlLexState::LineComment
                }
            }
            SqlLexState::BlockComment { depth } => {
                if rest.starts_with("/*") {
                    output.push_str("/*");
                    index += 2;
                    SqlLexState::BlockComment { depth: depth + 1 }
                } else if rest.starts_with("*/") {
                    output.push_str("*/");
                    index += 2;
                    if *depth <= 1 {
                        SqlLexState::Normal
                    } else {
                        SqlLexState::BlockComment { depth: depth - 1 }
                    }
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::BlockComment { depth: *depth }
                }
            }
            SqlLexState::DollarQuote { delimiter } => {
                if rest.starts_with(delimiter.as_str()) {
                    output.push_str(delimiter);
                    index += delimiter.len();
                    SqlLexState::Normal
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::DollarQuote {
                        delimiter: delimiter.clone(),
                    }
                }
            }
        };
        state = next;
    }

    match state {
        SqlLexState::Normal | SqlLexState::LineComment => Ok(output),
        SqlLexState::Single { .. } => Err("unterminated single-quoted SQL string".into()),
        SqlLexState::Double => Err("unterminated double-quoted SQL identifier".into()),
        SqlLexState::Backtick => Err("unterminated backtick-quoted SQL identifier".into()),
        SqlLexState::Bracket => Err("unterminated bracketed SQL identifier".into()),
        SqlLexState::BlockComment { .. } => Err("unterminated SQL block comment".into()),
        SqlLexState::DollarQuote { .. } => Err("unterminated SQL dollar quote".into()),
    }
}

fn safe_sql_literal(value: &Value) -> Result<String, String> {
    match value {
        Value::Number(number) => Ok(number.to_string()),
        Value::Bool(flag) => Ok(if *flag { "TRUE" } else { "FALSE" }.to_string()),
        Value::Null => Ok("NULL".to_string()),
        Value::String(_) => Err(
            "string template arguments are only allowed inside a single-quoted SQL literal".into(),
        ),
        Value::Array(_) | Value::Object(_) => {
            Err("array/object template arguments are not valid SQL literals".into())
        }
    }
}

fn sql_string_content(value: &Value, escape_backslash: bool) -> Result<String, String> {
    let rendered = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => {
            if *flag {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Value::Null => "NULL".to_string(),
        Value::Array(_) | Value::Object(_) => {
            return Err("array/object template arguments cannot be placed in a SQL string".into());
        }
    };
    Ok(escape_sql_string_content(&rendered, escape_backslash))
}

fn escape_sql_string_content(value: &str, escape_backslash: bool) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\'' => escaped.push_str("''"),
            '\\' if escape_backslash => escaped.push_str("\\\\"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn template_boundary_before(template: &str, index: usize) -> bool {
    let previous = template[..index].chars().next_back();
    !previous.is_some_and(is_sql_identifier_char) && previous != Some('.')
}

fn template_boundary_after(template: &str, index: usize) -> bool {
    let next = template[index..].chars().next();
    !next.is_some_and(is_sql_identifier_char) && next != Some('.')
}

fn is_sql_identifier_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '@'
}

fn has_postgres_escape_prefix(template: &str, quote_index: usize) -> bool {
    let before = &template[..quote_index];
    let mut chars = before.chars().rev();
    let Some(prefix) = chars.next() else {
        return false;
    };
    if !matches!(prefix, 'e' | 'E') {
        return false;
    }
    !chars.next().is_some_and(is_sql_identifier_char)
}

/// Recognise `$$` and `$tag$` dollar-quote openers at the start of `rest`.
fn dollar_quote_open(rest: &str) -> Option<(&str, usize)> {
    if !rest.starts_with('$') {
        return None;
    }
    if rest.starts_with("$$") {
        return Some((&rest[..2], 2));
    }
    let bytes = rest.as_bytes();
    if bytes.len() < 3 || !(bytes[1].is_ascii_alphabetic() || bytes[1] == b'_') {
        return None;
    }
    let mut end = 2;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    if bytes.get(end) == Some(&b'$') {
        Some((&rest[..=end], end + 1))
    } else {
        None
    }
}
