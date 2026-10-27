//! Conservative query-result mapping. SQL identifies a candidate relation only;
//! the driver must confirm it on the original pinned resource before verification.
use datazen::{
    execution::target::{ExecutionTarget, NamespaceTarget, ObjectTarget},
    shared::id::ConnectionId,
};
use datazen_driver_api::{ColumnInfo, TableSchema};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ResultRelationCandidate {
    /// Original quoted identifier path. Only the driver assigns namespace
    /// meanings, resolves unquoted casing and performs search_path lookup.
    pub relation_sql: String,
}

fn identifier(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    let first = input.chars().next()?;
    if matches!(first, '"' | '`' | '[') {
        let close = if first == '[' { ']' } else { first };
        let mut output = String::new();
        let mut chars = input[first.len_utf8()..].char_indices().peekable();
        while let Some((offset, character)) = chars.next() {
            if character == close {
                if chars.peek().is_some_and(|(_, next)| *next == close) {
                    chars.next();
                    output.push(close);
                    continue;
                }
                let end = first.len_utf8() + offset + close.len_utf8();
                return (!output.is_empty()).then_some((output, &input[end..]));
            }
            output.push(character);
        }
        return None;
    }
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let end = input
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '$')))
        .map(|(index, _)| index)
        .unwrap_or(input.len());
    Some((input[..end].into(), &input[end..]))
}

/// Exact wildcard, one relation, optional numeric LIMIT/OFFSET. Other legal
/// SELECT forms remain read-only until a richer origin proof is available.
pub fn result_relation_candidate(sql: &str) -> Option<ResultRelationCandidate> {
    let mut rest = sql.trim();
    let (word, tail) = identifier(rest)?;
    if !word.eq_ignore_ascii_case("select") {
        return None;
    }
    rest = tail.trim_start().strip_prefix('*')?.trim_start();
    let (word, tail) = identifier(rest)?;
    if !word.eq_ignore_ascii_case("from") {
        return None;
    }
    let mut names = Vec::new();
    rest = tail.trim_start();
    let relation_start = rest;
    loop {
        let (name, tail) = identifier(rest)?;
        names.push(name);
        rest = tail.trim_start();
        if let Some(tail) = rest.strip_prefix('.') {
            rest = tail;
        } else {
            break;
        }
        if names.len() >= 3 {
            return None;
        }
    }
    rest = rest.trim_end_matches(';').trim();
    if !rest.is_empty() {
        let words: Vec<&str> = rest.split_whitespace().collect();
        let valid = words.len() == 2
            && words[0].eq_ignore_ascii_case("limit")
            && words[1].parse::<u64>().is_ok()
            || words.len() == 4
                && words[0].eq_ignore_ascii_case("limit")
                && words[1].parse::<u64>().is_ok()
                && words[2].eq_ignore_ascii_case("offset")
                && words[3].parse::<u64>().is_ok();
        if !valid {
            return None;
        }
    }
    if names.is_empty() || names.len() > 3 {
        return None;
    }
    // Capture only the relation identifier path; LIMIT/OFFSET is not part of
    // the metadata request and must never reach the driver's identifier parser.
    let mut relation_end = relation_start;
    for index in 0..names.len() {
        let (_, tail) = identifier(relation_end)?;
        relation_end = tail;
        if index + 1 < names.len() {
            relation_end = tail.trim_start().strip_prefix('.')?;
        }
    }
    let length = relation_start.len() - relation_end.len();
    Some(ResultRelationCandidate {
        relation_sql: relation_start[..length].trim().to_owned(),
    })
}

/// `resolved` is driver-produced metadata from the SAME resource, including
/// pg_temp/search_path resolution. Never populate it from the editor dropdown.
pub fn verify_result_mapping(
    connection_id: ConnectionId,
    columns: &[ColumnInfo],
    schema: &TableSchema,
    resolved: &Value,
) -> Option<ExecutionTarget> {
    if columns.is_empty() || columns.len() != schema.columns.len() {
        return None;
    }
    if !columns
        .iter()
        .zip(&schema.columns)
        .all(|(result, source)| result.name == source.name)
    {
        return None;
    }
    let names: HashSet<&str> = columns.iter().map(|column| column.name.as_str()).collect();
    if names.len() != columns.len() {
        return None;
    }
    let primary_keys = schema.effective_primary_keys();
    if primary_keys.is_empty()
        || primary_keys
            .iter()
            .any(|name| !names.contains(name.as_str()))
    {
        return None;
    }
    let database = resolved.get("database")?.as_str()?.to_owned();
    let table = resolved.get("table")?.as_str()?.to_owned();
    if database.is_empty() || table.is_empty() {
        return None;
    }
    let schema = match resolved.get("schema")? {
        Value::Null => None,
        Value::String(name) if !name.is_empty() => Some(name.clone()),
        _ => return None,
    };
    let mut target = ExecutionTarget::new(
        connection_id,
        NamespaceTarget::new(Some(database), None, schema, Vec::new()),
    );
    target.object = Some(ObjectTarget::new("relation", table));
    Some(target)
}
