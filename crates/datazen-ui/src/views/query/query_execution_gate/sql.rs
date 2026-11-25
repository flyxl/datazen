//! Pure SQL helpers ported from the hook's dependencies: transaction guard,
//! bind-parameter parse/substitute, statement target resolution and the
//! Postgres double-quoted-literal heuristic. No command runs here.

/// Mask a region (string/comment) so scanners skip its bytes. Byte offsets are
/// kept so callers can slice the original source.
fn code_mask(sql: &str) -> Vec<bool> {
    let bytes = sql.as_bytes();
    let mut code = vec![true; bytes.len()];
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                code[i] = false;
                i += 1;
                while i < bytes.len() {
                    code[i] = false;
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        code[i + 1] = false;
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'\'' {
                        if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                            code[i + 1] = false;
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'"' => {
                code[i] = false;
                i += 1;
                while i < bytes.len() {
                    code[i] = false;
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                code[i] = false;
                code[i + 1] = false;
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    code[i] = false;
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                code[i] = false;
                code[i + 1] = false;
                i += 2;
                while i < bytes.len() {
                    code[i] = false;
                    if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                        code[i + 1] = false;
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    code
}

/// React `analyzeTransactionSql`: BEGIN/START TRANSACTION vs COMMIT/END/ROLLBACK.
#[allow(dead_code)] // pure helper; consumed by the reducer and tests.
pub fn transaction_analysis(sql: &str) -> (bool, usize, usize, i32) {
    let mask = code_mask(sql);
    let mut statements: Vec<String> = Vec::new();
    let mut start = 0usize;
    for (index, byte) in sql.bytes().enumerate() {
        if byte == b';' && mask.get(index).copied().unwrap_or(true) {
            let part = sql.get(start..index).unwrap_or("").trim();
            if !part.is_empty() {
                statements.push(part.to_ascii_uppercase());
            }
            start = index + 1;
        }
    }
    let tail = sql.get(start..).unwrap_or("").trim();
    if !tail.is_empty() {
        statements.push(tail.to_ascii_uppercase());
    }
    let (mut begin, mut end, mut depth) = (0usize, 0usize, 0i32);
    for head in statements {
        let normalized = head.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized == "BEGIN" || normalized.starts_with("BEGIN ") || normalized.starts_with("START TRANSACTION") {
            begin += 1;
            depth += 1;
        } else if normalized == "COMMIT"
            || normalized.starts_with("COMMIT ")
            || normalized == "END"
            || normalized.starts_with("END ")
            || (normalized.starts_with("ROLLBACK") && !normalized.starts_with("ROLLBACK TO "))
        {
            end += 1;
            depth = (depth - 1).max(0);
        }
    }
    (depth > 0, begin, end, depth)
}

/// React `hasSuspiciousPostgresDoubleQuotedLiteral`: `= "text"` etc. where the
/// quoted token is not an identifier (`"tbl"."col"`).
#[allow(dead_code)] // pure helper; consumed by the reducer and tests.
pub fn suspicious_pg_double_quote(sql: &str) -> bool {
    let operators = ["=", "<>", "!=", "LIKE", "ILIKE"];
    let bytes = sql.as_bytes();
    let lower = sql.to_ascii_lowercase();
    let mut search = 0usize;
    while search < sql.len() {
        let mut found: Option<(usize, usize)> = None;
        for op in &operators {
            let needle = op.to_ascii_lowercase();
            if let Some(at) = lower.get(search..).and_then(|tail| tail.find(&needle)) {
                let absolute = search + at;
                if found.is_none_or(|(start, _)| absolute < start) {
                    found = Some((absolute, absolute + op.len()));
                }
            }
        }
        let Some((op_at, mut i)) = found else { break };
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'"' {
            search = op_at + 1;
            continue;
        }
        let open = i;
        let mut close = open + 1;
        while close < bytes.len() && bytes[close] != b'"' {
            close += 1;
        }
        if close >= bytes.len() {
            return false;
        }
        let before = if open > 0 { bytes[open - 1] } else { b' ' };
        let after = if close + 1 < bytes.len() { bytes[close + 1] } else { b' ' };
        if before != b'.' && after != b'.' {
            return true;
        }
        search = close + 1;
    }
    false
}

/// One bind placeholder, mirroring `SqlParam` + its original-syntax label.
#[allow(dead_code)] // VM mirror; consumed by validation and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlParam {
    pub name: String,
    pub stable_id: String,
    pub label: String,
    pub from: usize,
    pub to: usize,
}

#[allow(dead_code)] // pure helper; consumed by the validator and tests.
pub fn parse_params(sql: &str) -> Vec<SqlParam> {
    let mask = code_mask(sql);
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut question = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if !mask.get(i).copied().unwrap_or(true) {
            i += 1;
            continue;
        }
        let is_ident_start = |b: u8| b.is_ascii_alphabetic() || b == b'_';
        let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        let read_ident = |from: usize| -> Option<usize> {
            let mut j = from;
            while j < bytes.len() && is_ident(bytes[j]) {
                j += 1;
            }
            if j > from { Some(j) } else { None }
        };
        match bytes[i] {
            b':' if i + 1 < bytes.len() && bytes[i + 1] != b':' => {
                if let Some(to) = read_ident(i + 1) {
                    let name = sql.get(i + 1..to).unwrap_or("").to_owned();
                    out.push(SqlParam {
                        label: format!(":{name}"),
                        stable_id: format!("named:{name}"),
                        name,
                        from: i,
                        to,
                    });
                    i = to;
                    continue;
                }
            }
            b'@' if i + 1 < bytes.len() && bytes[i + 1] != b'@' => {
                if let Some(to) = read_ident(i + 1) {
                    let name = sql.get(i + 1..to).unwrap_or("").to_owned();
                    out.push(SqlParam {
                        label: format!("@{name}"),
                        stable_id: format!("named:{name}"),
                        name,
                        from: i,
                        to,
                    });
                    i = to;
                    continue;
                }
            }
            b'$' if i + 2 < bytes.len() && bytes[i + 1] == b'{' => {
                if let Some(to) = read_ident(i + 2) {
                    if bytes.get(to) == Some(&b'}') {
                        let name = sql.get(i + 2..to).unwrap_or("").to_owned();
                        out.push(SqlParam {
                            label: format!("${{{name}}}"),
                            stable_id: format!("named:{name}"),
                            name,
                            from: i,
                            to: to + 1,
                        });
                        i = to + 1;
                        continue;
                    }
                }
            }
            b'$' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > i + 1 {
                    let name = sql.get(i + 1..j).unwrap_or("").to_owned();
                    out.push(SqlParam {
                        label: format!("${name}"),
                        stable_id: format!("dollar:{name}"),
                        name,
                        from: i,
                        to: j,
                    });
                    i = j;
                    continue;
                }
            }
            b'?' => {
                question += 1;
                out.push(SqlParam {
                    label: "?".to_owned(),
                    stable_id: format!("question:{question}"),
                    name: question.to_string(),
                    from: i,
                    to: i + 1,
                });
            }
            _ => {}
        }
        // Suppress a lint warning for the unused helper on dialects that never
        // reach it while keeping the predicate shared by all syntaxes.
        let _ = is_ident_start;
        i += 1;
    }
    out
}

/// React `findMissingParams`: a value is missing when absent, null or blank.
#[allow(dead_code)] // pure helper; consumed by the reducer and tests.
pub fn missing_params(sql: &str, values: &std::collections::BTreeMap<String, String>) -> Vec<SqlParam> {
    parse_params(sql)
        .into_iter()
        .filter(|param| {
            values
                .get(&param.stable_id)
                .or_else(|| values.get(&param.name))
                .or_else(|| values.get(param.label.trim_start_matches([':', '@', '$', '{', '}'])))
                .map_or(true, |value| value.trim().is_empty())
        })
        .collect()
}

/// React `substituteSqlParams` for the documented value formats.
#[allow(dead_code)] // pure helper; consumed by the reducer and tests.
pub fn substitute_params(sql: &str, values: &std::collections::BTreeMap<String, String>) -> String {
    let params = parse_params(sql);
    if params.is_empty() {
        return sql.to_owned();
    }
    let mut result = sql.to_owned();
    for param in params.into_iter().rev() {
        let raw = values
            .get(&param.stable_id)
            .or_else(|| values.get(&param.name))
            .or_else(|| values.get(param.label.trim_start_matches([':', '@', '$', '{', '}'])))
            .map(String::as_str)
            .unwrap_or("");
        let trimmed = raw.trim();
        let formatted = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
            "NULL".to_owned()
        } else if trimmed.eq_ignore_ascii_case("true") {
            "TRUE".to_owned()
        } else if trimmed.eq_ignore_ascii_case("false") {
            "FALSE".to_owned()
        } else if trimmed.parse::<f64>().is_ok() {
            trimmed.to_owned()
        } else {
            format!("'{}'", trimmed.replace('\'', "''"))
        };
        result.replace_range(param.from..param.to, &formatted);
    }
    result
}

/// Top-level statement ranges `(from, to)` in the original source.
#[allow(dead_code)] // pure helper; consumed by target resolution and tests.
pub fn statement_ranges(sql: &str) -> Vec<(usize, usize)> {
    let mask = code_mask(sql);
    let mut ranges = Vec::new();
    let mut start = 0usize;
    for (index, byte) in sql.bytes().enumerate() {
        if byte == b';' && mask.get(index).copied().unwrap_or(true) {
            // React's statement target keeps the terminating semicolon.
            let end = index + 1;
            if end > start {
                ranges.push((start, end));
            }
            start = index + 1;
        }
    }
    let end = sql.get(start..).unwrap_or("").trim_end().len() + start;
    if end > start {
        ranges.push((start, end));
    }
    ranges
}
