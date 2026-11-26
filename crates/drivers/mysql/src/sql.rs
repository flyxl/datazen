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

fn mysql_top_level_words(sql: &str) -> Option<Vec<(String, sqlparser::tokenizer::Location)>> {
    use sqlparser::{
        dialect::MySqlDialect,
        tokenizer::{Token, Tokenizer},
    };
    let tokens = Tokenizer::new(&MySqlDialect {}, sql)
        .tokenize_with_location()
        .ok()?;
    let mut depth = 0usize;
    let mut words = Vec::new();
    for token in tokens {
        match token.token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            Token::Word(word) if depth == 0 && word.quote_style.is_none() => {
                words.push((word.value.to_ascii_uppercase(), token.span.start));
            }
            _ => {}
        }
    }
    Some(words)
}

fn mysql_cte_returns_rows(words: &[(String, sqlparser::tokenizer::Location)]) -> bool {
    words
        .iter()
        .find_map(|(word, _)| match word.as_str() {
            "SELECT" | "TABLE" | "VALUES" => Some(true),
            "UPDATE" | "DELETE" | "INSERT" | "REPLACE" => Some(false),
            _ => None,
        })
        .unwrap_or(false)
}

fn mysql_sql_offset(sql: &str, location: sqlparser::tokenizer::Location) -> Option<usize> {
    let mut offset = 0;
    for (line, text) in sql.split_inclusive('\n').enumerate() {
        if line as u64 + 1 == location.line {
            let column = usize::try_from(location.column.checked_sub(1)?).ok()?;
            return text
                .char_indices()
                .nth(column)
                .map(|(byte, _)| offset + byte);
        }
        offset += text.len();
    }
    None
}

pub(crate) fn is_mysql_result_query(sql: &str) -> bool {
    let main = leading_sql(sql);
    let upper = main.to_ascii_uppercase();
    upper.starts_with("SELECT")
        || (upper.starts_with("WITH")
            && mysql_top_level_words(main).is_some_and(|words| mysql_cte_returns_rows(&words)))
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
    if !upper.starts_with("SELECT") && !upper.starts_with("WITH") {
        return (stmt.to_string(), None);
    }
    let Some(words) = mysql_top_level_words(trimmed) else {
        // Keep the native dialect text intact rather than guessing at a rewrite.
        return (stmt.to_string(), Some(lim));
    };
    if upper.starts_with("WITH") && !mysql_cte_returns_rows(&words) {
        return (stmt.to_string(), None);
    }
    if words.iter().any(|(word, _)| word == "LIMIT") {
        return (stmt.to_string(), Some(lim));
    }

    // MySQL's row locks come after LIMIT. Preserve native text (including
    // comments and optimizer hints), and insert at the clause's token offset.
    let lock = words.windows(2).find_map(|pair| {
        let (word, location) = &pair[0];
        match (word.as_str(), pair[1].0.as_str()) {
            ("FOR", "UPDATE" | "SHARE") | ("LOCK", "IN") => mysql_sql_offset(trimmed, *location),
            _ => None,
        }
    });
    let extra = lim.saturating_add(1);
    let effective = if let Some(offset) = lock {
        format!(
            "{}\nLIMIT {extra}\n{}",
            &trimmed[..offset],
            &trimmed[offset..]
        )
    } else {
        // A trailing -- or # comment must not absorb the server-side bound.
        format!("{trimmed}\nLIMIT {extra}")
    };
    (effective, Some(lim))
}
