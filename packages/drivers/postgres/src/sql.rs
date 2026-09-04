//! SQL parsing helpers for PostgreSQL driver.

use datazen_driver_api::{sql_dump, QueryResult, Value};

/// Split `schema.table` into `(Some(schema), table)`. Unqualified names use `None`.
pub(crate) fn parse_pg_table_ref(table: &str) -> (Option<&str>, &str) {
    match table.split_once('.') {
        Some((schema, name)) if !schema.is_empty() && !name.is_empty() => (Some(schema), name),
        _ => (None, table),
    }
}

pub(crate) fn pg_regclass_name(schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(s) => format!("{s}.{table}"),
        None => table.to_string(),
    }
}

pub(crate) fn is_pg_result_query(sql: &str) -> bool {
    let upper = sql.trim().to_ascii_uppercase();
    upper.starts_with("SELECT")
        || upper.starts_with("WITH")
        || upper.starts_with("SHOW")
        || upper.starts_with("EXPLAIN")
}

pub(crate) fn collect_named_ddl_column(
    result: &QueryResult,
    col_name: &str,
    kind_label: &str,
) -> String {
    let ddl_idx = result
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(col_name));
    let name_idx = result
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("name"));
    let Some(ddl_idx) = ddl_idx else {
        return String::new();
    };
    let mut out = String::new();
    for row in &result.rows {
        let Some(Value::String(ddl)) = row.get(ddl_idx).and_then(|v| v.as_ref()) else {
            continue;
        };
        if ddl.trim().is_empty() {
            continue;
        }
        if let Some(name_idx) = name_idx {
            if let Some(Value::String(name)) = row.get(name_idx).and_then(|v| v.as_ref()) {
                out.push_str(&format!("-- {kind_label}: {name}\n"));
            }
        }
        let trimmed = ddl.trim_end();
        out.push_str(trimmed);
        if !trimmed.ends_with(';') {
            out.push(';');
        }
        out.push_str("\n\n");
    }
    out
}

/// If the statement is a SELECT without an existing LIMIT clause, returns a
/// modified SQL with `LIMIT limit+1` appended (the extra row lets us detect
/// truncation).  If the statement already has a LIMIT, the SQL is unchanged
/// but the cap is still returned so the caller can truncate over-limit results.
pub(crate) fn apply_select_limit(stmt: &str, limit: Option<u32>) -> (String, Option<u32>) {
    let Some(lim) = limit else {
        return (stmt.to_string(), None);
    };

    let trimmed = stmt.trim();
    let upper = trimmed.to_ascii_uppercase();
    let is_select = upper.starts_with("SELECT") || upper.starts_with("WITH");
    if !is_select {
        return (stmt.to_string(), None);
    }

    if has_top_level_limit(trimmed) {
        return (stmt.to_string(), Some(lim));
    }

    let effective = format!("{} LIMIT {}", trimmed, lim + 1);
    (effective, Some(lim))
}

/// Rough heuristic: scan the SQL outside of string literals, dollar-quotes,
/// and parenthesised sub-expressions for the keyword `LIMIT`.
pub(crate) fn has_top_level_limit(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut depth: i32 = 0; // parenthesis nesting

    while i < len {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\'' {
                        i += 1;
                        if i < len && bytes[i] == b'\'' {
                            i += 1; // escaped quote
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'"' => {
                i += 1;
                while i < len && bytes[i] != b'"' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
            }
            b'$' => {
                if let Some(tag_end) = sql_dump::find_dollar_tag(bytes, i) {
                    let tag = &sql[i..tag_end];
                    i = tag_end;
                    loop {
                        if i >= len {
                            break;
                        }
                        if bytes[i] == b'$' {
                            if sql[i..].starts_with(tag) {
                                i += tag.len();
                                break;
                            }
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            b'-' if i + 1 < len && bytes[i + 1] == b'-' => {
                i += 2;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                i += 2;
                let mut cd = 1i32;
                while i + 1 < len && cd > 0 {
                    if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        cd += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        cd -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            b'L' | b'l' if depth == 0 => {
                if i + 5 <= len
                    && sql[i..i + 5].eq_ignore_ascii_case("LIMIT")
                    && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
                    && (i + 5 >= len || !bytes[i + 5].is_ascii_alphanumeric())
                {
                    return true;
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    false
}
