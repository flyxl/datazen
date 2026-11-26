//! MySQL SQL splitting, result-query detection, and SELECT LIMIT helpers.

/// Split SQL into statements, respecting strings, comments, and backtick identifiers.
pub(crate) fn split_mysql_statements(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut stmts: Vec<String> = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < len {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\'' {
                        i += 1;
                        if i < len && bytes[i] == b'\'' {
                            i += 1;
                        } else {
                            break;
                        }
                    } else if bytes[i] == b'\\' {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'"' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'"' {
                        i += 1;
                        if i < len && bytes[i] == b'"' {
                            i += 1;
                        } else {
                            break;
                        }
                    } else if bytes[i] == b'\\' {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'`' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'`' {
                        i += 1;
                        if i < len && bytes[i] == b'`' {
                            i += 1;
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'-' if bytes.get(i + 1) == Some(&b'-')
                && bytes.get(i + 2).is_some_and(u8::is_ascii_whitespace) =>
            {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'#' => {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < len {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            b';' => {
                let fragment = input[start..i].trim();
                if !leading_sql(fragment).is_empty() {
                    stmts.push(fragment.to_string());
                }
                i += 1;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }

    let tail = input[start..].trim();
    if !leading_sql(tail).is_empty() {
        stmts.push(tail.to_string());
    }
    stmts
}

fn leading_sql(mut sql: &str) -> &str {
    loop {
        sql = sql.trim_start();
        // MySQL executes version comments; unlike ordinary comments they
        // must reach the server even when they form the whole statement.
        if sql.starts_with("/*!") || sql.starts_with("/*M!") {
            return sql;
        }
        if let Some(comment) = sql.strip_prefix("/*") {
            let Some((_, tail)) = comment.split_once("*/") else {
                return sql; // let MySQL report the unterminated comment
            };
            sql = tail;
        } else if sql.starts_with('#')
            || (sql.starts_with("--") && sql.as_bytes().get(2).is_some_and(u8::is_ascii_whitespace))
        {
            sql = sql.split_once('\n').map_or("", |(_, tail)| tail);
        } else {
            return sql;
        }
    }
}

pub(crate) fn is_mysql_result_query(sql: &str) -> bool {
    let upper = leading_sql(sql).to_ascii_uppercase();
    upper.starts_with("SELECT")
        || upper.starts_with("WITH")
        || upper.starts_with("SHOW")
        || upper.starts_with("DESCRIBE")
        || upper.starts_with("DESC")
        || upper.starts_with("EXPLAIN")
}

pub(crate) fn apply_mysql_select_limit(stmt: &str, limit: Option<u32>) -> (String, Option<u32>) {
    let Some(lim) = limit else {
        return (stmt.to_string(), None);
    };

    let trimmed = stmt.trim();
    let upper = leading_sql(trimmed).to_ascii_uppercase();
    let is_select = upper.starts_with("SELECT") || upper.starts_with("WITH");
    if !is_select {
        return (stmt.to_string(), None);
    }

    use sqlparser::{
        dialect::MySqlDialect,
        tokenizer::{Token, Tokenizer},
    };
    let Ok(tokens) = Tokenizer::new(&MySqlDialect {}, trimmed).tokenize() else {
        // Keep the native dialect text intact rather than guessing at a rewrite.
        return (stmt.to_string(), Some(lim));
    };
    let mut depth = 0usize;
    for token in tokens {
        match token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            Token::Word(word)
                if depth == 0
                    && word.quote_style.is_none()
                    && word.value.eq_ignore_ascii_case("LIMIT") =>
            {
                return (stmt.to_string(), Some(lim));
            }
            _ => {}
        }
    }

    // A trailing -- or # comment must not absorb the server-side bound.
    let effective = format!("{}\nLIMIT {}", trimmed, lim.saturating_add(1));
    (effective, Some(lim))
}
