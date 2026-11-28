//! Conservative names-only completion for a native SELECT's first FROM.
use datazen_ui_api::{
    BaseTableSqlGenerator, Catalog, SqlCompletionKind, infer_sql_completion_kind,
    last_bare_sql_keyword_with_offset,
};

use super::model::{COMPLETION_LIMIT, starts_with_ignore_ascii_case, word_start};

pub(super) fn identifier_continuation(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '.' | '$' | '"' | '`' | '[')
}

pub(super) fn from_prefix(doc: &str, head: usize, explicit: bool) -> Option<(usize, &str)> {
    let before = doc.get(..head)?;
    let after = doc.get(head..)?;
    if before.len() > 8_192
        || after.chars().next().is_some_and(identifier_continuation)
    {
        return None;
    }
    let start = word_start(doc, head);
    let prefix = before.get(start..)?;
    if prefix.is_empty() {
        if !explicit {
            return None;
        }
    } else if !prefix.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_') {
        return None;
    }
    let left = before[..start].trim_start();
    let rest = left.get(6..)?;
    if !left.get(..6)?.eq_ignore_ascii_case("SELECT")
        || !rest.chars().next().is_some_and(char::is_whitespace)
    {
        return None;
    }
    // This pilot deliberately doesn't infer complex/nested scope, comments,
    // quoted text or SQL outside a single simple SELECT.
    if rest.contains([';', '\'', '"', '`', '$', '\\', '[', ']'])
        || rest.contains("--") || rest.contains("/*") || rest.contains("*/")
        || (explicit && rest.contains('#'))
    {
        return None;
    }
    let mut depth = 0usize;
    for ch in rest.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    if depth != 0 || infer_sql_completion_kind(before) != SqlCompletionKind::Table {
        return None;
    }
    let (keyword, char_offset) = last_bare_sql_keyword_with_offset(left)?;
    let tail: String = left.chars().skip(char_offset).collect();
    if keyword != "from" || tail.is_empty() || !tail.chars().all(char::is_whitespace) {
        return None;
    }
    let end_of_from = left.char_indices().nth(char_offset).map_or(left.len(), |(byte, _)| byte);
    let projection = left.get(6..end_of_from.checked_sub(4)?)?;
    if projection.trim().is_empty()
        || projection.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
            .any(|word| word.eq_ignore_ascii_case("from") || word.eq_ignore_ascii_case("join"))
    {
        return None;
    }
    Some((start, prefix))
}

pub(super) fn relations(catalog: &Catalog, database: &str, prefix: &str) -> Vec<String> {
    let Some(node) = catalog.databases.iter().find(|db| db.name == database) else {
        return Vec::new();
    };
    let quote = BaseTableSqlGenerator::new("\"");
    let mut items: Vec<String> = node
        .schemas
        .iter()
        .flat_map(|schema| {
            schema.tables.iter().chain(&schema.views)
                .filter(move |table| starts_with_ignore_ascii_case(table, prefix))
                .map(|table| (schema.name.as_str(), table.as_str()))
        })
        .map(|(schema, table)| {
            if schema.is_empty() {
                quote.quote(table)
            } else {
                format!("{}.{}", quote.quote(schema), quote.quote(table))
            }
        })
        .collect();
    items.sort_unstable();
    items.dedup();
    items.truncate(COMPLETION_LIMIT);
    items
}
