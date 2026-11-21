mod types;
#[cfg(test)]
mod tests;

pub use types::*;

use std::collections::{BTreeMap, BTreeSet};

struct ResolvedPolicy<'a> {
    enable_at: bool,
    enable_question: bool,
    enable_dollar_positional: bool,
    enable_dollar_named: bool,
    enable_template: bool,
    filter: Option<&'a SqlParamFilter>,
}

impl<'a> ResolvedPolicy<'a> {
    fn new(policy: Option<&'a SqlParamDialectPolicy>) -> Self {
        let (at, question, dollar_positional, dollar_named, template, filter) = match policy {
            Some(p) => (
                p.enable_at,
                p.enable_question,
                p.enable_dollar_positional,
                p.enable_dollar_named,
                p.enable_template,
                p.filter_occurrences.as_deref(),
            ),
            None => (None, None, None, None, None, None),
        };
        Self {
            enable_at: at.unwrap_or(DEFAULT_ENABLE_AT),
            enable_question: question.unwrap_or(DEFAULT_ENABLE_QUESTION),
            enable_dollar_positional: dollar_positional
                .unwrap_or(DEFAULT_ENABLE_DOLLAR_POSITIONAL),
            enable_dollar_named: dollar_named.unwrap_or(DEFAULT_ENABLE_DOLLAR_NAMED),
            enable_template: template.unwrap_or(DEFAULT_ENABLE_TEMPLATE),
            filter,
        }
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn read_ident(bytes: &[u8], start: usize) -> Option<usize> {
    if start >= bytes.len() || !is_ident_start(bytes[start]) {
        return None;
    }
    let mut j = start + 1;
    while j < bytes.len() && is_ident_char(bytes[j]) {
        j += 1;
    }
    Some(j)
}

fn read_digits(bytes: &[u8], start: usize) -> Option<usize> {
    if start >= bytes.len() || !bytes[start].is_ascii_digit() {
        return None;
    }
    let mut j = start + 1;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    Some(j)
}

fn skip_quote(sql: &str, i: usize) -> usize {
    let bytes = sql.as_bytes();
    let quote = bytes[i];
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == quote {
            if quote == b'\'' && j + 1 < bytes.len() && bytes[j + 1] == b'\'' {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    bytes.len()
}

fn skip_dollar_quote(sql: &str, i: usize) -> usize {
    let bytes = sql.as_bytes();
    if bytes.get(i) != Some(&b'$') {
        return i;
    }
    let mut j = i + 1;
    while j < bytes.len() && (is_ident_char(bytes[j]) || bytes[j] == b'_') {
        j += 1;
    }
    if j >= bytes.len() || bytes[j] != b'$' {
        return i;
    }
    let tag = &sql[i..=j];
    match sql[j + 1..].find(tag) {
        Some(off) => j + 1 + off + tag.len(),
        None => bytes.len(),
    }
}

fn skip_non_code(sql: &str, i: usize) -> usize {
    let bytes = sql.as_bytes();
    let ch = match bytes.get(i) {
        Some(b) => *b,
        None => return i,
    };
    if ch == b'\'' || ch == b'"' || ch == b'`' {
        return skip_quote(sql, i);
    }
    if ch == b'-' && bytes.get(i + 1) == Some(&b'-') {
        return match sql[i..].find('\n') {
            Some(off) => i + off + 1,
            None => bytes.len(),
        };
    }
    if ch == b'/' && bytes.get(i + 1) == Some(&b'*') {
        return match sql[i + 2..].find("*/") {
            Some(off) => i + 2 + off + 2,
            None => bytes.len(),
        };
    }
    if ch == b'$' {
        return skip_dollar_quote(sql, i);
    }
    i
}

fn named_id(name: &str) -> SqlParamStableId {
    format!("named:{}", name)
}

fn dollar_id(n: u64) -> SqlParamStableId {
    format!("dollar:{}", n)
}

fn question_id(n: usize) -> SqlParamStableId {
    format!("question:{}", n)
}

#[allow(clippy::too_many_arguments)]
fn push_occurrence(
    list: &mut Vec<SqlParamOccurrence>,
    sql: &str,
    from: usize,
    to: usize,
    id: SqlParamStableId,
    syntax: SqlParamSyntax,
    name: &str,
    ordinal: Option<usize>,
) {
    list.push(SqlParamOccurrence {
        from,
        to,
        token: sql[from..to].to_owned(),
        id,
        syntax,
        name: name.to_owned(),
        ordinal,
    });
}

pub fn parse_sql_param_occurrences(
    sql: &str,
    policy: Option<&SqlParamDialectPolicy>,
) -> Vec<SqlParamOccurrence> {
    let resolved = ResolvedPolicy::new(policy);
    let mut occurrences = Vec::new();
    let mut question_ordinal = 0usize;
    let mut i = 0usize;
    let bytes = sql.as_bytes();

    while i < bytes.len() {
        let skipped = skip_non_code(sql, i);
        if skipped != i {
            i = skipped;
            continue;
        }

        let ch = bytes[i];

        if ch == b':' {
            if bytes.get(i + 1) == Some(&b':') {
                i += 2;
                continue;
            }
            if let Some(end) = read_ident(bytes, i + 1) {
                let name = &sql[i + 1..end];
                let id = named_id(name);
                push_occurrence(
                    &mut occurrences,
                    sql,
                    i,
                    end,
                    id,
                    SqlParamSyntax::Colon,
                    name,
                    None,
                );
                i = end;
                continue;
            }
        }

        if ch == b'@' && resolved.enable_at {
            if bytes.get(i + 1) == Some(&b'@') {
                i += 2;
                continue;
            }
            if let Some(end) = read_ident(bytes, i + 1) {
                let name = &sql[i + 1..end];
                let id = named_id(name);
                push_occurrence(
                    &mut occurrences,
                    sql,
                    i,
                    end,
                    id,
                    SqlParamSyntax::At,
                    name,
                    None,
                );
                i = end;
                continue;
            }
        }

        if ch == b'$' {
            if resolved.enable_template && bytes.get(i + 1) == Some(&b'{') {
                if let Some(end) = read_ident(bytes, i + 2) {
                    if bytes.get(end) == Some(&b'}') {
                        let name = &sql[i + 2..end];
                        let id = named_id(name);
                        push_occurrence(
                            &mut occurrences,
                            sql,
                            i,
                            end + 1,
                            id,
                            SqlParamSyntax::Template,
                            name,
                            None,
                        );
                        i = end + 1;
                        continue;
                    }
                }
            }
            if resolved.enable_dollar_named {
                if let Some(end) = read_ident(bytes, i + 1) {
                    let name = &sql[i + 1..end];
                    let id = named_id(name);
                    push_occurrence(
                        &mut occurrences,
                        sql,
                        i,
                        end,
                        id,
                        SqlParamSyntax::DollarNamed,
                        name,
                        None,
                    );
                    i = end;
                    continue;
                }
            }
            if resolved.enable_dollar_positional {
                if let Some(end) = read_digits(bytes, i + 1) {
                    let name = &sql[i + 1..end];
                    let num = name.parse::<u64>().unwrap_or(0);
                    let id = dollar_id(num);
                    push_occurrence(
                        &mut occurrences,
                        sql,
                        i,
                        end,
                        id,
                        SqlParamSyntax::DollarPositional,
                        name,
                        None,
                    );
                    i = end;
                    continue;
                }
            }
        }

        if ch == b'?' && resolved.enable_question {
            question_ordinal += 1;
            let name = question_ordinal.to_string();
            let id = question_id(question_ordinal);
            push_occurrence(
                &mut occurrences,
                sql,
                i,
                i + 1,
                id,
                SqlParamSyntax::Question,
                &name,
                Some(question_ordinal),
            );
            i += 1;
            continue;
        }

        i += 1;
    }

    let Some(filter) = resolved.filter else {
        return occurrences;
    };
    let accepted = filter(sql, &occurrences);
    let keys: BTreeSet<(usize, usize, SqlParamStableId)> = accepted
        .into_iter()
        .map(|o| (o.from, o.to, o.id))
        .collect();
    occurrences
        .into_iter()
        .filter(|o| keys.contains(&(o.from, o.to, o.id.clone())))
        .collect()
}

pub fn parse_sql_params(sql: &str, policy: Option<&SqlParamDialectPolicy>) -> Vec<SqlParam> {
    let mut seen = BTreeSet::new();
    let mut params = Vec::new();
    for occ in parse_sql_param_occurrences(sql, policy) {
        if !seen.insert(occ.id.clone()) {
            continue;
        }
        params.push(SqlParam {
            name: occ.name,
            kind: if occ.syntax.is_positional() {
                SqlParamKind::Positional
            } else {
                SqlParamKind::Named
            },
            syntax: occ.syntax,
            stable_id: occ.id,
            ordinal: occ.ordinal,
        });
    }
    params
}

fn lookup<'a>(raw: &'a BTreeMap<String, ParamValue>, key: &str) -> Option<&'a ParamValue> {
    raw.get(key).filter(|v| !matches!(v, ParamValue::Null))
}

fn missing_value() -> ParamValue {
    ParamValue::Str(String::new())
}

pub fn build_bind_payload_v2(
    sql: &str,
    raw_values: &BTreeMap<String, ParamValue>,
    policy: Option<&SqlParamDialectPolicy>,
) -> SqlBindPayloadV2 {
    let occurrences = parse_sql_param_occurrences(sql, policy);
    let mut values = BTreeMap::new();
    let mut payload_occurrences = Vec::new();
    for occ in occurrences {
        let raw = lookup(raw_values, &occ.id)
            .or_else(|| lookup(raw_values, &occ.name))
            .cloned()
            .unwrap_or_else(missing_value);
        values.insert(occ.id.clone(), coerce_param_value(&raw));
        payload_occurrences.push(SqlParamOccurrenceV2 {
            from: occ.from,
            to: occ.to,
            id: occ.id,
            token: occ.token,
        });
    }
    SqlBindPayloadV2 {
        version: 2,
        values,
        occurrences: payload_occurrences,
    }
}

pub fn params_to_payload(
    params: &[SqlParam],
    values: &BTreeMap<String, ParamValue>,
) -> BTreeMap<String, ParamValue> {
    let mut out = BTreeMap::new();
    for p in params {
        let raw = lookup(values, &p.stable_id)
            .or_else(|| lookup(values, &p.name))
            .cloned()
            .unwrap_or_else(missing_value);
        out.insert(p.name.clone(), coerce_param_value(&raw));
    }
    out
}

fn format_param_value(raw: Option<&ParamValue>) -> String {
    match raw {
        None => "NULL".to_owned(),
        Some(ParamValue::Null) => "NULL".to_owned(),
        Some(ParamValue::Bool(b)) => {
            if *b {
                "TRUE".to_owned()
            } else {
                "FALSE".to_owned()
            }
        }
        Some(ParamValue::Number(n)) => format!("{}", n),
        Some(ParamValue::Str(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
                "NULL".to_owned()
            } else if trimmed.eq_ignore_ascii_case("true") {
                "TRUE".to_owned()
            } else if trimmed.eq_ignore_ascii_case("false") {
                "FALSE".to_owned()
            } else if is_decimal_number(trimmed) {
                trimmed.to_owned()
            } else {
                format!("'{}'", s.replace('\'', "''"))
            }
        }
    }
}

pub fn substitute_sql_params(
    sql: &str,
    raw_values: &BTreeMap<String, ParamValue>,
    policy: Option<&SqlParamDialectPolicy>,
) -> String {
    let mut occurrences = parse_sql_param_occurrences(sql, policy);
    if occurrences.is_empty() {
        return sql.to_owned();
    }
    occurrences.sort_by(|a, b| b.from.cmp(&a.from));
    let mut result = sql.to_owned();
    for occ in occurrences {
        let raw = lookup(raw_values, &occ.id)
            .or_else(|| lookup(raw_values, &occ.name))
            .or_else(|| occ.ordinal.and_then(|n| lookup(raw_values, &n.to_string())))
            .or_else(|| occ.name.strip_prefix('$').and_then(|s| lookup(raw_values, s)))
            .or_else(|| occ.name.strip_prefix('@').and_then(|s| lookup(raw_values, s)))
            .or_else(|| occ.name.strip_prefix(':').and_then(|s| lookup(raw_values, s)));
        let formatted = format_param_value(raw);
        result.replace_range(occ.from..occ.to, &formatted);
    }
    result
}
